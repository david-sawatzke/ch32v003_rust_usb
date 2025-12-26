#include "rv003usb.h"
// Taken from ch32v003fun

#define PERIPH_BASE                             (0x40000000) /* Peripheral base address in the alias region */
#define APB2PERIPH_BASE                         (PERIPH_BASE + 0x10000)
#define EXTI_BASE                               (APB2PERIPH_BASE + 0x0400)
#define GPIOA_BASE                              (APB2PERIPH_BASE + 0x0800)
#define GPIOC_BASE                              (APB2PERIPH_BASE + 0x1000)
#define GPIOD_BASE                              (APB2PERIPH_BASE + 0x1400)
#define TIM1_BASE                               (APB2PERIPH_BASE + 0x2C00)

// Done, now stuff taken from rv003usb follows

#define CFGLR_OFFSET 0
#define INDR_OFFSET 8
#define BSHR_OFFSET 16


#define SYSTICK_CNT 0xE000F008

// This is 6 * n + 3 cycles
#define nx6p3delay( n, freereg ) li freereg, ((n)+1); 1: c.addi freereg, -1; c.bnez freereg, 1b

//See RV003USB_DEBUG_TIMING note in .c file.
#define DEBUG_TICK_SETUP la x4, (TIM1_BASE + 0x58) 	// for debug (Go nowhere)
#define DEBUG_TICK_MARK .balign 4; sw x0, 0(x4)
#define	RESTORE_DEBUG_MARKER(x) lw	x4, x(sp)
#define SAVE_DEBUG_MARKER(x)	sw	x4, x(sp)

.global test_memory
.global rv003usb_internal_data
.global rv003usb_handle_packet
.global main
// Temp globals as we move to rust
.global done_usb_message_in
.global ret_from_se0

/* Register map
	zero, ra, sp, gp, tp, t0, t1, t2
Compressed:
	s0, s1,	a0, a1, a2, a3, a4, a5
*/

.section .text.vector_handler
.global EXTI7_0_IRQHandler

.balign 4
EXTI7_0_IRQHandler:
	addi	sp,sp,-80
	sw	a0, 0(sp)
	sw	a5, 20(sp)
	la a5, USB_GPIO_BASE
	c.lw a0, INDR_OFFSET(a5) // MUST check SE0 immediately.
	c.andi a0, USB_DMASK

	sw	a1, 4(sp)
	sw	a2, 8(sp)
	sw	a3, 12(sp)
	sw	a4, 16(sp)
	sw	s1, 28(sp)

	SAVE_DEBUG_MARKER( 48 );
	DEBUG_TICK_SETUP
	c.lw a1, INDR_OFFSET(a5)
	c.andi a1, USB_DMASK;

	// Finish jump to se0
	c.beqz a0, handle_se0_keepalive

	c.lw a0, INDR_OFFSET(a5); c.andi a0, USB_DMASK; bne a0, a1, syncout
	c.lw a0, INDR_OFFSET(a5); c.andi a0, USB_DMASK; bne a0, a1, syncout
	c.lw a0, INDR_OFFSET(a5); c.andi a0, USB_DMASK; bne a0, a1, syncout
	c.lw a0, INDR_OFFSET(a5); c.andi a0, USB_DMASK; bne a0, a1, syncout
	c.lw a0, INDR_OFFSET(a5); c.andi a0, USB_DMASK; bne a0, a1, syncout
	c.lw a0, INDR_OFFSET(a5); c.andi a0, USB_DMASK; bne a0, a1, syncout
	c.lw a0, INDR_OFFSET(a5); c.andi a0, USB_DMASK; bne a0, a1, syncout
	c.j syncout
syncout:
	sw	s0, 24(sp)
	li a2, 0
	sw	t0, 32(sp)  // XXX NOTE: This is actually unused register - remove some day?
	sw	t1, 36(sp)

	// We are coarsely sync'd here.

	// This will be called when we have synchronized our USB.  We can put our
	// preamble detect code here.  But we have a whole free USB bit cycle to
	// do whatever we feel like.

	// A little weird, but this way, the USB packet is always aligned.
#define DATA_PTR_OFFSET (59+4)
	
	// This is actually somewhat late.
	// The preamble loop should try to make it earlier.
.balign 4
preamble_loop:
	DEBUG_TICK_MARK
	c.lw a0, INDR_OFFSET(a5);
	c.andi a0, USB_DMASK;
	c.beqz a0, done_usb_message // SE0 here?
	c.xor a0, a1;
	c.xor a1, a0; // Recover a1.
	j 1f; 1: // 4 cycles?
	c.beqz a0, done_preamble
	j 1f; 1: // 4 cycles?
	c.lw s0, INDR_OFFSET(a5);
	c.andi s0, USB_DMASK;
	c.xor s0, a1

	// TRICKY: This helps retime the USB sync.
	// If s0 is nonzero, then it's changed (we're going too slow)
	c.bnez s0, 2f;  // This code takes 6 cycles or 8 cycles, depending.
	c.j 1f; 1:
	2:
	j preamble_loop // 4 cycles
.balign 4
done_preamble:
	sw  t2, 40(sp)
	sw  ra, 52(sp)
	// 16-byte temporary buffer at 56+sp

	// XXX TODO: Do one byte here to determine the header byte and from that set the CRC.
	c.li s1, 8

	// This is the first bit that matters.
	c.li s0, 6 // 1 runs.

	c.nop; 

	// 8 extra cycles here cause errors.
	// -5 cycles is too much.
	// -4 to +6 cycles is OK

	//XXX NOTE: It actually wouldn't be too bad to insert an *extra* cycle here.

	/* register meanings:
		* x4 = TP = used for triggering debug.

		* T0 = Totally unushed.
		* T1 = TEMPORARY
		* T2 = Pointer to the memory address we are writing to.

		* A0 = temp / current bit value.
		* A1 = last-frame's GPIO values.
		* A2 = The running word 
		* A3 = Running CRC
		* a4 = Polynomial
		* A5 = GPIO Offset

		* S0 = Bit Stuff Place
		* S1 = # output bits remaining.
*/

.balign 4
packet_type_loop:
	// Up here to delay loop a tad, and we need to execute them anyway.
	// TODO: Maybe we could further sync bits here instead of take up time?
	// I.e. can we do what we're doing above, here, and take less time, but sync
	// up when possible.
	li a3, 0xffff // Starting CRC of 0.   Because USB doesn't respect reverse CRCing.
	li a4, 0xa001
	addi  t2, sp, DATA_PTR_OFFSET //rv003usb_internal_data
	la  t0, 0x80
	c.nop

	DEBUG_TICK_MARK
	c.lw a0, INDR_OFFSET(a5);
	c.andi a0, USB_DMASK;
	c.beqz a0, done_usb_message // Not se0 complete, that can't happen here and be valid.
	c.xor a0, a1;
	c.xor a1, a0; // Recover a1, for next cycle
	// a0 = 00 for 1 and 11 for 0

	// No CRC for the header.
	//c.srli a0, USB_PIN_DP
	//c.addi a0, 1 // 00 -> 1, 11 -> 100
	//c.andi a0, 1 // If 1, 1 if 0, 0
        c.nop
        seqz a0, a0

	// Write header into byte in reverse order, because we can.
	c.slli a2, 1
	c.or a2, a0

	// Handle bit stuffing rules.
	c.addi a0, -1 // 0->0xffffffff 1->0
	c.or s0, a0
	c.andi s0, 7
	c.addi s0, -1
	c.addi s1, -1
	c.bnez s1, packet_type_loop

///////////////////////////////////////////////////////////////////////////////////////
///////////////////////////////////////////////////////////////////////////////////////	

	// XXX Here, figure out CRC polynomial.

	li s1, (USB_BUFFER_SIZE*8) // # of bits we culd read.

	// WARNING: a0 is bit-wise backwards here.
	// 0xb4 for instance is a setup packet.
	//
	// When we get here, packet type is loaded in A2.
	// If packet type is 0xXX01 or 0xXX11
	// the LSBs are the inverted packet type.
	// we can branch off of bit 2.
	andi a0, a2, 0x0c

	// if a0 is 1 then it's DATA (full CRC) otherwise,
	// (0) for setup or PARTIAL CRC.
	// Careful:  This has to take a constant amount of time either way the branch goes.
	c.beqz a0, data_crc
	c.li a4, 0x14	
	c.li a3, 0x1e
	.word 0x00000013 // nop, for alignment of data_crc.
data_crc:


#define HANDLE_EOB_YES \
	sb a2, 0(t2); /* Save the byte off. TODO: Is unaligned byte access to RAM slow? */ \
	.word 0x00138393; /*addi t2, t2, 1;*/

///////////////////////////////////////////////////////////////////////////////////////
///////////////////////////////////////////////////////////////////////////////////////	
.balign 4
is_end_of_byte:
	HANDLE_EOB_YES

	// end-of-byte.
.balign 4
bit_process:
	DEBUG_TICK_MARK
	c.lw a0, INDR_OFFSET(a5);
	c.andi a0, USB_DMASK;
	c.xor a0, a1;

//XXX GOOD
#define HANDLE_NEXT_BYTE(is_end_of_byte, jumptype)  \
	c.addi s1, -1; \
	andi a0, s1, 7; /* s1 could be really really big */ \
	c.jumptype a0, is_end_of_byte /* 4 cycles for this section. (Checked) (Sometimes 5)? */ 

	c.beqz a0, handle_one_bit
handle_zero_bit:
	c.xor a1, a0; // Recover a1, for next cycle

	// TODO: Do we have time to do time fixup here?
	// Can we resync time here?
	// If they are different, we need to sloowwww dowwwnnn
	// There is some free time.  Could do something interesting here!!!
	// I was thinking we could put the resync code here.
	c.j 1f; 1:  //Delay 4 cycles.

	c.li s0, 6      // reset runs-of-one.
	c.beqz a1, se0_complete

	// Handle CRC (0 bit)  (From @Domkeykong)
	slli a0,a3,31 // Put a3s LSB into a0s MSB
	c.srai a0,31    // Copy MSB into all other bits
	c.srli a3,1
	c.and  a0, a4
	c.xor  a3, a0

	c.srli a2, 1;  // shift a2 down by 1
	HANDLE_NEXT_BYTE(is_end_of_byte, beqz)
	c.nop
	c.nop
	c.nop
	c.bnez s1, bit_process // + 4 cycles
	c.j done_usb_message


.balign 4
handle_one_bit:
	c.addi s0, -1; // Count # of runs of 1 (subtract 1)
	//HANDLE_CRC (1 bit)
	andi a0, a3, 1
	c.addi a0, -1
	c.and a0, a4
	c.srli a3, 1
	c.xor a3, a0

	c.srli a2, 1;  // shift a2 down by 1
	ori a2, a2, 0x80
	c.beqz s0, handle_bit_stuff;

	HANDLE_NEXT_BYTE(is_end_of_byte, beqz)
	c.nop // Need extra delay here because we need more time if it's end-of-byte.
	c.nop
	c.nop
	c.bnez s1, bit_process // + 4 cycles
	c.j done_usb_message

handle_bit_stuff:
	// We want to wait a little bit, then read another byte, and make
	// sure everything is well, before heading back into the main loop
	// Debug blip
	HANDLE_NEXT_BYTE(not_is_end_of_byte_and_bit_stuffed, bnez)
	HANDLE_EOB_YES

not_is_end_of_byte_and_bit_stuffed:
        DEBUG_TICK_MARK
	c.lw a0, INDR_OFFSET(a5);
	c.andi a0, USB_DMASK;
	c.beqz a0, se0_complete
	c.xor a0, a1;
	c.xor a1, a0; // Recover a1, for next cycle.

	// If A0 is a 0 then that's bad, we just did a bit stuff
        //   and A0 == 0 means there was no signal transition
	c.beqz a0, done_usb_message

        // Reset bit stuff, delay, then continue onto the next actual bit
	c.li s0, 6;

        c.nop;
	nx6p3delay( 2, a0 )

	c.bnez s1, bit_process // + 4 cycles

.balign 4
se0_complete:
	// This is triggered when we finished getting a packet.
	andi a0, s1, 7; // Make sure we received an even number of bytes.
	c.bnez a0, done_usb_message



	// Special: handle ACKs?
	// Now we have to decide what we're doing based on the
	// packet type.
	addi  a1, sp, DATA_PTR_OFFSET
	lbu  a0, 0(a1)
	c.addi a1, 1

	// 0010 => 01001011 => ACK
	// 0011 => 11000011 => DATA0
	// 1011 => 11010010 => DATA1
	// 1001 => 10010110 => PID IN
	// 0001 => 10000111 => PID_OUT
	// 1101 => 10110100 => SETUP    (OK)

	// a0 contains first 4 bytes.
	la ra, done_usb_message_in // Common return address for all function calls.

	// For ACK don't worry about CRC.
	addi a5, a0, -0b01001011

	RESTORE_DEBUG_MARKER(48) // restore x4 for whatever in C land.

	la a4, rv003usb_internal_data

	// ACK doesn't need good CRC.
	c.beqz a5, usb_pid_handle_ack

	// Next, check for tokens.
	c.bnez a3, crc_for_tokens_would_be_bad_maybe_data
may_be_a_token:
	// Our CRC is 0, so we might be a token.

	// Do token-y things.
	lhu a2, 0(a1)
	andi a0, a2, 0x7f // addr
	c.srli a2, 7
	c.andi a2, 0xf    // endp
	li s0, ENDPOINTS
	bgeu a2, s0, done_usb_message // Make sure < ENDPOINTS
	c.beqz a0,  yes_check_tokens
	// Otherwise, we might have our assigned address.
	lbu s0, MY_ADDRESS_OFFSET_BYTES(a4)
	bne s0, a0, done_usb_message // addr != 0 && addr != ours.
yes_check_tokens:
	addi a5, a5, (0b01001011-0b10000111)
	c.beqz a5, usb_pid_handle_out
	c.addi a5, (0b10000111-0b10010110)
	c.beqz a5, usb_pid_handle_in
	c.addi a5, (0b10010110-0b10110100)
	c.beqz a5, usb_pid_handle_setup

	c.j done_usb_message_in

	// CRC is nonzero. (Good for Data packets)
crc_for_tokens_would_be_bad_maybe_data:
	li s0, 0xb001  // UGH: You can't use the CRC16 in reverse :(
	c.sub a3, s0
	c.bnez a3, done_usb_message_in
	// Good CRC!!
	sub a3, t2, a1 //a3 = # of bytes read..
	c.addi a3, 1
	addi a5, a5, (0b01001011-0b11000011)
	c.li a2, 0
	c.beqz a5, usb_pid_handle_data
	c.addi a5, (0b11000011-0b11010010)
	c.li a2, 1
	c.beqz a5, usb_pid_handle_data

done_usb_message:
done_usb_message_in:
	lw	s0, 24(sp)
	lw	s1, 28(sp)
	lw	t0, 32(sp)
	lw	t1, 36(sp)
	lw	t2, 40(sp)
	lw  ra, 52(sp)


ret_from_se0:
	lw	s1, 28(sp)
	RESTORE_DEBUG_MARKER(48)
	lw	a2, 8(sp)
	lw	a3, 12(sp)
	lw	a4, 16(sp)
	lw	a1, 4(sp)

interrupt_complete:
	// Acknowledge interrupt.
	// EXTI->INTFR = 1<<4
	c.j 1f; 1: // Extra little bit of delay to make sure we don't accidentally false fire.

	la a5, EXTI_BASE + 20
	li a0, (1<<USB_PIN_DM)
	sw a0, 0(a5)
	
	// Restore stack.
	lw	a0, 0(sp)
	lw	a5, 20(sp)
	addi	sp,sp,80
	mret
