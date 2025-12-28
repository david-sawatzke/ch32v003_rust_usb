// Almost fully copied from rv003usb
// MIT License

// Copyright (c) 2023 CNLohr

// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:

// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.

// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.
use ch32_hal::pac::{FLASH, PFIC, RCC};
use core::hint::unreachable_unchecked;
use core::mem;

const ENDPOINT0_SIZE: u32 = 8;

pub struct UsbEndpoint {
    count: u32,
    toggle_in: u32,
    toggle_out: u32,
    custom: u32,
    max_len: u32,
    _reserved1: u32,
    _reserved2: u32,
    opaque: *const u8,
}
impl UsbEndpoint {
    const fn new() -> Self {
        Self {
            count: 0,
            toggle_in: 0,
            toggle_out: 0,
            custom: 0,
            max_len: 0,
            _reserved1: 0,
            _reserved2: 0,
            opaque: core::ptr::null(),
        }
    }
}

#[repr(C, packed)]
struct UsbUrb {
    w_request_type_lsb_request_msb: u16,
    l_value_lsb_index_msb: u32,
    w_length: u16,
}

pub struct UsbIf<const USB_BASE: usize, const DP: u8, const DM: u8, const EPS: usize> {
    current_endpoint: u32,
    my_address: u32,
    setup_request: u32,
    reboot_armed: u32,
    last_se0_cyccount: u32,
    delta_se0_cyccount: i32,
    se0_windup: u32,
    usb_handle_user_in_request: fn(*mut UsbEndpoint, *mut u8, i32, u32, &mut Self),
    get_descriptor_info: fn(u32) -> (*const u8, u16),
    eps: [UsbEndpoint; EPS], // ENDPOINTS
}

impl<const USB_BASE: usize, const DP: u8, const DM: u8, const EPS: usize>
    UsbIf<USB_BASE, DP, DM, EPS>
{
    pub fn new(
        usb_handle_user_in_request: fn(*mut UsbEndpoint, *mut u8, i32, u32, &mut Self),
        get_descriptor_info: fn(u32) -> (*const u8, u16),
    ) -> Self {
        Self {
            current_endpoint: 0,
            my_address: 0,
            setup_request: 0,
            reboot_armed: 0,
            last_se0_cyccount: 0,
            delta_se0_cyccount: 0,
            se0_windup: 0,
            usb_handle_user_in_request,
            get_descriptor_info,
            eps: [const { UsbEndpoint::new() }; EPS],
        }
    }
    #[unsafe(naked)]
    pub(crate) unsafe extern "C" fn handle_se0_keepalive(&mut self) {
        // NOTE: this code can *almost* be converted to rust
        // With the single exception that t0-t2 aren't saved by our caller,
        // which is in a performance critical section, so doesn't *quite* follow
        // the normal calling convention
        core::arch::naked_asm!(
            // In here, we want to do smart stuff with the
            // 1ms tick.
            "la a4, {SYSTICK_CNT}",
            // self is in a0
            "c.lw a1, {LAST_SE0_OFFSET}(a0)", //last cycle count   last_se0_cyccount
            "c.lw a2, 0(a4)", //this cycle count
            "c.sw a2, {LAST_SE0_OFFSET}(a0)", //store it back to last_se0_cyccount
            "c.sub a2, a1",
            "c.sw a2, {DELTA_SE0_OFFSET}(a0)", //record delta_se0_cyccount
            "li a1, 48000",
            "c.sub a2, a1",
            // This is our deviance from 48MHz.

            // Make sure we aren't in left field.
            "li a5, 4000",
            "bge a2, a5, 1f",
            "li a5, -4000",
            "blt a2, a5, 1f",
            "c.lw a1, {SE0_WINDUP_OFFSET}(a0)", // load windup se0_windup
            "c.add a1, a2",
            "c.sw a1, {SE0_WINDUP_OFFSET}(a0)", // save windup
            // No further adjustments
            "beqz a1, 1f",
            "la a4, {RCC_CTRL}",
            "lw a0, 0(a4)",
            "srli a2, a0, 3", // Extract HSI Trim.
            "andi a2, a2, 0b11111",
            "li a5, 0xffffff07",
            "and a0, a0, a5", // Mask off non-HSI
            // Decimate windup - use as HSI-Trim.
            "neg a1, a1
            srai a2, a1, 9",
            "addi a2, a2, 16  // add HSI offset.",
            // Put trim in place in register.
            "slli a2, a2, 3",
            "or a0, a0, a2",
            "sw a0, 0(a4)",
            "1:",
            "ret",
            SE0_WINDUP_OFFSET = const mem::offset_of!(Self, se0_windup),
            LAST_SE0_OFFSET = const mem::offset_of!(Self, last_se0_cyccount),
            DELTA_SE0_OFFSET = const mem::offset_of!(Self, delta_se0_cyccount),
            RCC_CTRL = const 0x40021000, // RCC.CTRL
            SYSTICK_CNT = const 0xE000F008 as u32,

        );
        // TODO get periph register addresses from/to proper addr
    }

    pub(crate) fn usb_send_empty(&mut self, token: u32) {
        unsafe { self.usb_send_data(&[0_u8, 0_u8] as *const u8, 2, 2, token) };
    }

    #[allow(named_asm_labels)]
    #[unsafe(naked)]
    pub(crate) unsafe extern "C" fn usb_send_data(
        &mut self,
        data: *const u8,
        length: u32,
        poly_function: u32,
        token: u32,
    ) {
        core::arch::naked_asm!(
            ".macro nx6p3delay n, freereg",
            "li \\freereg, ((\\n) + 1)",
            "1: c.addi \\freereg, -1",
            "c.bnez \\freereg, 1b",
            ".endm",
            // Needed ...
            ".balign 4",
            "addi	sp,sp,-16",
            "sw	s0, 0(sp)",
            "sw	s1, 4(sp)",
            // TODO shuffle this properly
            "mv a0, a1",
            "mv a1, a2",
            "mv a2, a3",
            "mv a3, a4",

            "la a5, {USB_GPIO_BASE}",

            // ASAP: Turn the bus around and send our preamble + token.
            "c.lw a4, {CFGLR_OFFSET}(a5)",

            "li s1, ~((0b1111<<({USB_PIN_DP}*4)) | (0b1111<<({USB_PIN_DM}*4)))",
            "and a4, s1, a4",

            // Convert D+/D- into 2MHz outputs
            "li s1, ((0b0010<<({USB_PIN_DP}*4)) | (0b0010<<({USB_PIN_DM}*4)))",
            "or a4, s1, a4",

            "li s1, (1<<{USB_PIN_DP}) | (1<<({USB_PIN_DM}+16))",
            "c.sw s1, {BSHR_OFFSET}(a5)",

            //00: Universal push-pull output mode
            "c.sw a4, {CFGLR_OFFSET}(a5)",

            "li t1, (1<<{USB_PIN_DP}) | (1<<({USB_PIN_DM}+16)) | (1<<{USB_PIN_DM}) | (1<<({USB_PIN_DP}+16))",


            // Save off our preamble and token.
            "c.slli a3, 7", //Put token further up so it gets sent later.
            "ori s0, a3, 0x40",

            "li t0, 0x0000",
            "c.bnez a2, done_poly_check",
            "li t0, 0xa001",
            "li a2, 0xffff",
            "done_poly_check:",

            "c.slli a1, 3", // bump up one extra to be # of bits
            "mv t2, a1",

            // t0 is our polynomial
            // a2 is our running CRC.
            // a3 is our token.

            "c.li a4, 6", // reset bit stuffing.
            "c.li a1, 15", // 15 bits.

            //c.nop; c.nop; c.nop;
            "c.j pre_and_tok_send_inner_loop",

            ////////////////////////////////////////////////////////////////////////////
            // Send preamble + token
            ".balign 4",
            "pre_and_tok_send_inner_loop:",
            /* High-level:
            * extract the LSB from s0
            * If it's 0, we FLIP the USB pin state.
            * If it's 1, we don't flip.
            * Regardless of the state, we still compute CRC.
            * We have to decrement our bit stuffing index.
            * If it is 0, we can reset our bit stuffing index.
            */

            // a3 is now the lsb of s0 (the 'next bit' to read out)
            "c.mv a3, s0",
            "c.srli s0, 1",  // Shift down into the next bit.
            "c.andi a3, 1",
            // If a3 is 0, we should FLIP
            // if a3 is 1, we should NOT flip.

            "c.addi a4, -1",
            "c.bnez a3, pre_and_tok_send_one_bit",
            //pre_and_tok_send_one_bit:
            //Send 0 bit. (Flip)
            // Flip s1 (our bshr setting) by xoring it.
            // 10.....01
            // 11.....11 (xor with)
            // 01.....10
            "xor s1, s1, t1",
            "c.li a4, 6", // reset bit stuffing.
            // DO NOT flip.  Allow a4 to increment.
            // Deliberately unaligned for timing purposes.
            ".balign 4",
            "pre_and_tok_send_one_bit:",
            "sw s1, {BSHR_OFFSET}(a5)",
            //Bit stuffing doesn't happen.
            "c.addi a1, -1",
            "c.beqz a1, pre_and_tok_done_sending_data",
            "nx6p3delay 2, a3 ;	c.nop", // Free time!
            "c.j pre_and_tok_send_inner_loop",
            ".balign 4",
            "pre_and_tok_done_sending_data:",
            ////////////////////////////////////////////////////////////////////////////

            // We have very little time here.  Just enough to do this.

            //Restore size.
            "mv a1, t2//lw  a1, 12(sp)",
            "c.beqz a1, no_really_done_sending_data", //No actual payload?  Bail!
            "c.addi a1, -1",
            //	beqz t2, no_really_done_sending_data

            "bnez t0, done_poly_check2",
            "li a2, 0xffff",
            "done_poly_check2:",


            // t0 is used for CRC
            // t1 is free
            // t2 is a backup of size.

            // s1 is our last "state\"
            //    bit 0 is last "physical" state,
            // s0 is our current "bit" / byte / temp.

            // a0 is our data
            // a1 is our length
            // a2 our CRC
            // a3 is TEMPORARY
            // a4 is used for bit stuffing.
            // a5 is the output address.

            //xor s1, s1, t1
            //c.sw s1, BSHR_OFFSET(a5)

            // This creates a preamble, which is alternating 1's and 0's
            // and then it sets the same state.
            //	li s0, 0b10000000

            //	c.j send_inner_loop
            ".balign 4",
            "load_next_byte:",
            "lb s0, 0(a0)",
            ".long 0x00150513", // addi a0, a0, 1  (For alignment's sake)
            "send_inner_loop:",
            /* High-level:
            * extract the LSB from s0
            * If it's 0, we FLIP the USB pin state.
            * If it's 1, we don't flip.
            * Regardless of the state, we still compute CRC.
            * We have to decrement our bit stuffing index.
            * If it is 0, we can reset our bit stuffing index.
            */

            // a3 is now the lsb of s0 (the 'next bit' to read out)
            "c.mv a3, s0",
            "c.andi a3, 1",
            // If a3 is 0, we should FLIP
            // if a3 is 1, we should NOT flip.
            "c.beqz a3, send_zero_bit",
            "c.srli s0, 1", // Shift down into the next bit.
            //send_one_bit:
            //HANDLE_CRC (1 bit)
            "andi a3, a2, 1",
            "c.addi a3, -1",
            "and a3, a3, t0",
            "c.srli a2, 1",
            "c.xor a2, a3",

            "c.addi a4, -1",
            "c.beqz a4, insert_stuffed_bit",
            "c.j cont_after_jump",
            //Send 0 bit. (Flip)
            ".balign 4",
            "send_zero_bit:",
            "c.srli s0, 1", // Shift down into the next bit.

            // Handle CRC (0 bit)
            // a2 is our running CRC
            // a3 is temp
            // t0 is polynomial.

            // XXX WARNING: this was by https://github.com/cnlohr/rv003usb/issues/7
            // TODO Check me!
            "slli a3,a2,31", // Put a3s LSB into a0s MSB
            "c.srai a3,31", // Copy MSB into all other bits

            // Flip s1 (our bshr setting) by xoring it.
            // 10.....01
            // 11.....11 (xor with)
            // 01.....10
            "xor s1, s1, t1",
            "sw s1, {BSHR_OFFSET}(a5)",

            "c.li a4, 6", // reset bit stuffing.

            // XXX XXX CRC down here to make bit stuffing timings line up.
            "c.srli a2,1",
            "and a3,a3,t0",
            "c.xor  a2,a3",

            ".balign 4",
            "cont_after_jump:",
            "send_end_bit_complete:",
            "c.beqz a1, done_sending_data",
            "andi a3, a1, 7",
            "c.addi a1, -1",
            "c.beqz a3, load_next_byte",
            // Wait an extra few cycles.
            "c.j 1f; 1:",
            "c.j send_inner_loop",

            ".balign 4",
            "done_sending_data:",
            // BUT WAIT!! MAYBE WE NEED TO CRC!
            "beqz t0, no_really_done_sending_data",
            "srli t0, t0, 8", // reset poly - we don't want it anymore.
            "li a1, 7", // Load 8 more bits out
            "beqz t0, send_inner_loop", //Second CRC byte
            // First CRC byte
            "not s0, a2", // get read to send out the CRC.
            "c.j send_inner_loop",


            ".balign 4",
            "no_really_done_sending_data:",

            //	c.bnez a2, poly_function  TODO: Uncomment me!

            "nx6p3delay 2, a3",

            // Need to perform an SE0.
            "li s1, (1<<({USB_PIN_DM}+16)) | (1<<({USB_PIN_DP}+16))",
            "c.sw s1, {BSHR_OFFSET}(a5)",

            "nx6p3delay 7, a3",

            "li s1, (1<<({USB_PIN_DM})) | (1<<({USB_PIN_DP}+16))",
            "c.sw s1, {BSHR_OFFSET}(a5)",

            "lw s1, {CFGLR_OFFSET}(a5)",
            // Convert D+/D- into inputs.
            "li a3, ~((0b11<<({USB_PIN_DP}*4)) | (0b11<<({USB_PIN_DM}*4)))",
            "and s1, a3, s1",
            // 01: Floating input mode.
            "li a3, ((0b01<<({USB_PIN_DP}*4+2)) | (0b01<<({USB_PIN_DM}*4+2)))",
            "or s1, a3, s1",
            "sw s1, {CFGLR_OFFSET}(a5)",

            "lw	s0, 0(sp)",
            "lw	s1, 4(sp)",
            "addi	sp,sp,16",
            "ret",

            ".balign 4",
            // TODO: This seems to be either 222 or 226 (not 224) in cases.
            // It's off by 2 clock cycles.  Probably OK, but, hmm.
            "insert_stuffed_bit:",
            "nx6p3delay 3, a3",
            "xor s1, s1, t1",
            "c.li a4, 6", // reset bit stuffing.
            "c.nop",
            "c.nop",
            "sw s1, {BSHR_OFFSET}(a5)",
            "c.j send_end_bit_complete",
            // Restore original assembler state. "Needed" to ensure the macro
            // doesn't pollute other sections, but in practice (almost) never an
            // issue
            ".purgem nx6p3delay",
            USB_GPIO_BASE = const USB_BASE,
            USB_PIN_DP = const DP,
            USB_PIN_DM = const DM,
            BSHR_OFFSET = const 16, // Don't see a good way to get this from the pac?
            CFGLR_OFFSET = const 0,
        );
    }

    #[allow(named_asm_labels)]
    #[unsafe(naked)]
    pub(crate) unsafe extern "C" fn usb_interrupt_handler(&mut self) {
        // TODO this doesn't *need* to be an naked function
        // a few cycles of latency can be tolerated
        core::arch::naked_asm!(

        // This is 6 * n + 3 cycles
        ".macro nx6p3delay n, freereg",
            "li \\freereg, ((\\n) + 1)",
            "1: c.addi \\freereg, -1",
            "c.bnez \\freereg, 1b",
        ".endm",

        // UNUSED, legacy
        ".macro DEBUG_TICK_MARK",
            "c.nop",
            "c.nop",
        ".endm",

        /* Register map
        zero, ra, sp, gp, tp, t0, t1, t2
        Compressed:
        s0, s1,	a0, a1, a2, a3, a4, a5
        */

        ".balign 4",
        "addi	sp,sp,-80",
        "la a5, {USB_GPIO_BASE}",
        "c.lw a4, {INDR_OFFSET}(a5)", // MUST check SE0 immediately.
        "c.andi a4, {USB_DMASK}",

        "sw	ra, 52(sp)",
        "sw a0, 56(sp)",

        "c.lw a1, {INDR_OFFSET}(a5)",
        "c.andi a1, {USB_DMASK}",

        "la ra, ret_from_se0", // Common return address for all function calls.
        // Finish jump to se0
        "c.beqz a4, {handle_se0_keepalive}",

        "c.lw a0, {INDR_OFFSET}(a5); c.andi a0, {USB_DMASK}; bne a0, a1, syncout",
        "c.lw a0, {INDR_OFFSET}(a5); c.andi a0, {USB_DMASK}; bne a0, a1, syncout",
        "c.lw a0, {INDR_OFFSET}(a5); c.andi a0, {USB_DMASK}; bne a0, a1, syncout",
        "c.lw a0, {INDR_OFFSET}(a5); c.andi a0, {USB_DMASK}; bne a0, a1, syncout",
        "c.lw a0, {INDR_OFFSET}(a5); c.andi a0, {USB_DMASK}; bne a0, a1, syncout",
        "c.lw a0, {INDR_OFFSET}(a5); c.andi a0, {USB_DMASK}; bne a0, a1, syncout",
        "c.lw a0, {INDR_OFFSET}(a5); c.andi a0, {USB_DMASK}; bne a0, a1, syncout",
        "c.j syncout",
        "syncout:",
        "sw	s0, 24(sp)",
        "li a2, 0",
        "sw	t0, 32(sp)", // XXX NOTE: This is actually unused register - remove some day?
        "sw	t1, 36(sp)",

        // We are coarsely sync'd here.

        // This will be called when we have synchronized our USB.  We can put our
        // preamble detect code here.  But we have a whole free USB bit cycle to
        // do whatever we feel like.

        // This is actually somewhat late.
        // The preamble loop should try to make it earlier.
        ".balign 4",
        "preamble_loop:",
        "DEBUG_TICK_MARK",
        "c.lw a0, {INDR_OFFSET}(a5)",
        "c.andi a0, {USB_DMASK}",
        "c.beqz a0, done_usb_message", // SE0 here?
        "c.xor a0, a1",
        "c.xor a1, a0", // Recover a1.
        "j 1f; 1:", // 4 cycles?
        "c.beqz a0, done_preamble",
        "j 1f; 1:", // 4 cycles?
        "c.lw s0, {INDR_OFFSET}(a5)",
        "c.andi s0, {USB_DMASK}",
        "c.xor s0, a1",

        // TRICKY: This helps retime the USB sync.
        // If s0 is nonzero, then it's changed (we're going too slow)
        "c.bnez s0, 2f", // This code takes 6 cycles or 8 cycles, depending.
        "c.j 1f; 1:",
        "2:",
        "j preamble_loop", // 4 cycles
        ".balign 4",
        "done_preamble:",
        "sw  t2, 40(sp)",
        "sw  t2, 40(sp)", // DUMMY required for timing reasons
        // 16-byte temporary buffer at 56+sp

        // XXX TODO: Do one byte here to determine the header byte and from that set the CRC.
        "c.li s1, 8",

        // This is the first bit that matters.
        "c.li s0, 6", // 1 runs.

        "c.nop",

        // 8 extra cycles here cause errors.
        // -5 cycles is too much.
        // -4 to +6 cycles is OK

        //XXX NOTE: It actually wouldn't be too bad to insert an *extra* cycle here.

        /* register meanings:
            * x4 = TP = used for triggering debug
            * T0 = Totally unushed.",
            * T1 = TEMPORARY
            * T2 = Pointer to the memory address we are writing to
            * A0 = temp / current bit value.",
            * A1 = last-frame's GPIO values.
            * A2 = The running word
            * A3 = Running CRC
            * a4 = Polynomial
            * A5 = GPIO Offset
            * S0 = Bit Stuff Place",
            * S1 = # output bits remaining.
        */
        ".balign 4",
        "packet_type_loop:",
        // Up here to delay loop a tad, and we need to execute them anyway.
        // TODO: Maybe we could further sync bits here instead of take up time?
        // I.e. can we do what we're doing above, here, and take less time, but sync
        // up when possible.
        "li a3, 0xffff", // Starting CRC of 0.   Because USB doesn't respect reverse CRCing.
        "li a4, 0xa001",
        "addi  t2, sp, {DATA_PTR_OFFSET}",
        "la  t0, 0x80",
        "c.nop",

        "DEBUG_TICK_MARK",
        "c.lw a0, {INDR_OFFSET}(a5)",
        "c.andi a0, {USB_DMASK}",
        "c.beqz a0, done_usb_message", // Not se0 complete, that can't happen here and be valid.
        "c.xor a0, a1",
        "c.xor a1, a0", // Recover a1, for next cycle
        // a0 = 00 for 1 and 11 for 0

        // No CRC for the header.
        //c.srli a0, {USB_PIN_DP}
        //c.addi a0, 1 // 00 -> 1, 11 -> 100
        //c.andi a0, 1 // If 1, 1 if 0, 0
        "c.nop",
        "seqz a0, a0",

        // Write header into byte in reverse order, because we can.
        "c.slli a2, 1",
        "c.or a2, a0",

        // Handle bit stuffing rules.
        "c.addi a0, -1", // 0->0xffffffff 1->0
        "c.or s0, a0",
        "c.andi s0, 7",
        "c.addi s0, -1",
        "c.addi s1, -1",
        "c.bnez s1, packet_type_loop",

        ///////////////////////////////////////////////////////////////////////////////////////
        ///////////////////////////////////////////////////////////////////////////////////////

        // XXX Here, figure out CRC polynomial.

        "li s1, ({USB_BUFFER_SIZE}*8)", // # of bits we culd read.

        // WARNING: a0 is bit-wise backwards here.
        // 0xb4 for instance is a setup packet.
        //
        // When we get here, packet type is loaded in A2.
        // If packet type is 0xXX01 or 0xXX11
        // the LSBs are the inverted packet type.
        // we can branch off of bit 2.
        "andi a0, a2, 0x0c",

        // if a0 is 1 then it's DATA (full CRC) otherwise,
        // (0) for setup or PARTIAL CRC.
        // Careful:  This has to take a constant amount of time either way the branch goes.
        "c.beqz a0, data_crc",
        "c.li a4, 0x14	",
        "c.li a3, 0x1e",
        ".word 0x00000013", // nop, for alignment of data_crc.
        "data_crc:",


        ".macro HANDLE_EOB_YES",
        "sb a2, 0(t2)", /* Save the byte off. TODO: Is unaligned byte access to RAM slow? */
        ".word 0x00138393", /*addi t2, t2, 1;*/
        ".endm",

        ///////////////////////////////////////////////////////////////////////////////////////
        ///////////////////////////////////////////////////////////////////////////////////////
        ".balign 4",
        "is_end_of_byte:",
        "HANDLE_EOB_YES",

        // end-of-byte.
        ".balign 4",
        "bit_process:",
        "DEBUG_TICK_MARK",
        "c.lw a0, {INDR_OFFSET}(a5)",
        "c.andi a0, {USB_DMASK}",
        "c.xor a0, a1",

        //XXX GOOD
        ".macro HANDLE_NEXT_BYTE is_end_of_byte, jumptype",
        "c.addi s1, -1",
        "andi a0, s1, 7; /* s1 could be really really big */",
        "c.\\jumptype a0, \\is_end_of_byte", /* 4 cycles for this section. (Checked) (Sometimes 5)? */
        ".endm",

        "c.beqz a0, handle_one_bit",
        "handle_zero_bit:",
        "c.xor a1, a0", // Recover a1, for next cycle

        // TODO: Do we have time to do time fixup here?
        // Can we resync time here?
        // If they are different, we need to sloowwww dowwwnnn
        // There is some free time.  Could do something interesting here!!!
        // I was thinking we could put the resync code here.
        "c.j 1f; 1:", //Delay 4 cycles.

        "c.li s0, 6", // reset runs-of-one.
        "c.beqz a1, se0_complete",

        // Handle CRC (0 bit)  (From @Domkeykong)
        "slli a0,a3,31", // Put a3s LSB into a0s MSB
        "c.srai a0,31", // Copy MSB into all other bits
        "c.srli a3,1",
        "c.and  a0, a4",
        "c.xor  a3, a0",

        "c.srli a2, 1", // shift a2 down by 1
        "HANDLE_NEXT_BYTE is_end_of_byte, beqz",
        "c.nop",
        "c.nop",
        "c.nop",
        "c.bnez s1, bit_process", // + 4 cycles
        "c.j done_usb_message",


        ".balign 4",
        "handle_one_bit:",
        "c.addi s0, -1", // Count # of runs of 1 (subtract 1)
        //HANDLE_CRC (1 bit)
        "andi a0, a3, 1",
        "c.addi a0, -1",
        "c.and a0, a4",
        "c.srli a3, 1",
        "c.xor a3, a0",

        "c.srli a2, 1", // shift a2 down by 1
        "ori a2, a2, 0x80",
        "c.beqz s0, handle_bit_stuff",

        "HANDLE_NEXT_BYTE is_end_of_byte, beqz",
        "c.nop", // Need extra delay here because we need more time if it's end-of-byte.
        "c.nop",
        "c.nop",
        "c.bnez s1, bit_process", // + 4 cycles
        "c.j done_usb_message",

        "handle_bit_stuff:",
        // We want to wait a little bit, then read another byte, and make
        // sure everything is well, before heading back into the main loop
        // Debug blip
        "HANDLE_NEXT_BYTE not_is_end_of_byte_and_bit_stuffed, bnez",
        "HANDLE_EOB_YES",

        "not_is_end_of_byte_and_bit_stuffed:",
        "DEBUG_TICK_MARK",
        "c.lw a0, {INDR_OFFSET}(a5)",
        "c.andi a0, {USB_DMASK}",
        "c.beqz a0, se0_complete",
        "c.xor a0, a1",
        "c.xor a1, a0", // Recover a1, for next cycle.

        // If A0 is a 0 then that's bad, we just did a bit stuff
        //   and A0 == 0 means there was no signal transition
        "c.beqz a0, done_usb_message",

        // Reset bit stuff, delay, then continue onto the next actual bit
        "c.li s0, 6",

        "c.nop",
        "nx6p3delay 2, a0",

        "c.bnez s1, bit_process", // + 4 cycles

        ".balign 4",
        "se0_complete:",
        // This is triggered when we finished getting a packet.
        "andi a0, s1, 7", // Make sure we received an even number of bytes.
        "c.bnez a0, done_usb_message",

        // Special: handle ACKs?
        // Now we have to decide what we're doing based on the
        // packet type.
        "addi  a1, sp, {DATA_PTR_OFFSET}",
        "lbu  a0, 0(a1)",
        "c.addi a1, 1",

        // 0010 => 01001011 => ACK
        // 0011 => 11000011 => DATA0
        // 1011 => 11010010 => DATA1
        // 1001 => 10010110 => PID IN
        // 0001 => 10000111 => PID_OUT
        // 1101 => 10110100 => SETUP    (OK)

        // a0 contains first 4 bytes.
        "la ra, done_usb_message_in", // Common return address for all function calls.

        // For ACK don't worry about CRC.
        "addi a5, a0, -0b01001011",


        // Shuffle registers around
        // TODO do this before
        "mv a4, a3",
        "mv a3, a2",
        "mv a2, a1",
        "mv a1, a0",
        // Recover self pointer from previous
        "lw a0, 56(sp)",

        // ACK doesn't need good CRC.
        "c.beqz a5, {usb_pid_handle_ack}",

        // Next, check for tokens.
        "c.bnez a4, crc_for_tokens_would_be_bad_maybe_data",
        //"may_be_a_token:", //Label unused
        // Our CRC is 0, so we might be a token.

        // Do token-y things.
        "lhu a3, 0(a2)",
        "andi a1, a3, 0x7f", // addr
        "c.srli a3, 7",
        "c.andi a3, 0xf", // endp
        "li s0, {ENDPOINTS}",
        "bgeu a3, s0, done_usb_message", // Make sure < ENDPOINTS
        "c.beqz a1,  yes_check_tokens",
        // Otherwise, we might have our assigned address.
        "lbu s0, {MY_ADDRESS_OFFSET_BYTES}(a0)",
        "bne s0, a1, done_usb_message", // addr != 0 && addr != ours.
        "yes_check_tokens:",
        "addi a5, a5, (0b01001011-0b10000111)",
        "c.beqz a5, {usb_pid_handle_out}",
        "c.addi a5, (0b10000111-0b10010110)",
        "c.beqz a5, {usb_pid_handle_in}",
        "c.addi a5, (0b10010110-0b10110100)",
        "c.beqz a5, {usb_pid_handle_setup}",

        "c.j done_usb_message_in",

        // CRC is nonzero. (Good for Data packets)
        "crc_for_tokens_would_be_bad_maybe_data:",
        "li s0, 0xb001", // UGH: You can't use the CRC16 in reverse :(
        "c.sub a4, s0",
        "c.bnez a4, done_usb_message_in",
        // Good CRC!!
        "sub a4, t2, a2", //a4 = # of bytes read..
        "c.addi a4, 1",
        "addi a5, a5, (0b01001011-0b11000011)",
        "c.li a3, 0",
        "c.beqz a5, {usb_pid_handle_data}",
        "c.addi a5, (0b11000011-0b11010010)",
        "c.li a3, 1",
        "c.beqz a5, {usb_pid_handle_data}",

        "done_usb_message:",
        "done_usb_message_in:",
        "lw	s0, 24(sp)",
        "lw	s1, 28(sp)",


        "ret_from_se0:",

        "interrupt_complete:",
        "lw ra, 52(sp)",
        // Acknowledge interrupt.
        // EXTI->INTFR = 1<<4
        "c.j 1f; 1:", // Extra little bit of delay to make sure we don't accidentally false fire.

        "la a5, {EXTI_BASE} + 20",
        "li a0, (1<<{USB_PIN_DM})",
        "sw a0, 0(a5)",

        // Restore stack.
        "addi	sp,sp,80",
        "ret",

        ".purgem nx6p3delay",
        ".purgem HANDLE_EOB_YES",
        ".purgem HANDLE_NEXT_BYTE",
        ".purgem DEBUG_TICK_MARK",

            USB_GPIO_BASE = const USB_BASE,
            USB_PIN_DM = const DM,
            // A little weird, but this way, the USB packet is always aligned.
            DATA_PTR_OFFSET = const 59+4,
            MY_ADDRESS_OFFSET_BYTES = const mem::offset_of!(Self, my_address),
            INDR_OFFSET = const 8,
            USB_DMASK = const ((1<<(DP)) | 1<<(DM)),
            USB_BUFFER_SIZE = const 12, // Packet Type + 8 + CRC + Buffer
            ENDPOINTS = const EPS, // TODO make configurable
            // NOTE the following is *not* compile time defined
            // EXTI_BASE = const (EXTI.as_ptr()) as u32,
            EXTI_BASE = const 0x40010400,
            usb_pid_handle_data = sym Self::usb_pid_handle_data,
            usb_pid_handle_in = sym Self::usb_pid_handle_in,
            usb_pid_handle_out = sym Self::usb_pid_handle_out,
            usb_pid_handle_ack = sym Self::usb_pid_handle_ack,
            usb_pid_handle_setup = sym Self::usb_pid_handle_setup,
            handle_se0_keepalive = sym Self::handle_se0_keepalive,
        );
    }

    extern "C" fn usb_pid_handle_in(&mut self, _addr: u32, data: *mut u8, endp: u32) {
        self.current_endpoint = endp;

        let e = &mut self.eps[endp as usize];
        let sendtok = if e.toggle_in != 0 {
            0b01001011
        } else {
            0b11000011
        };
        if self.reboot_armed == 2 {
            self.usb_send_empty(sendtok);

            // Initiate boot into bootloader
            FLASH.boot_modekeyp().write(|w| w.set_modekeyr(0x45670123)); // FLASH_KEY1
            FLASH.boot_modekeyp().write(|w| w.set_modekeyr(0xCDEF89AB)); // FLASH_KEY2
            FLASH.statr().write(|w| w.set_boot_mode(true)); // 1<<14 is zero, so, boot bootloader code. Unset for user code.

            FLASH.ctlr().write(|w| w.set_lock(true));
            RCC.rstsckr().modify(|w| w.set_rmvf(true));
            // reset here
            PFIC.sctlr().write(|w| w.set_sysreset(true));
            unsafe { unreachable_unchecked() };
        }
        if (e.custom != 0) || (endp != 0) {
            (self.usb_handle_user_in_request)(e, data, endp as i32, sendtok, self);
            return;
        }
        let tsend = e.opaque;
        let offset = e.count << 3;
        let tosend = if (e.max_len - offset) > ENDPOINT0_SIZE {
            ENDPOINT0_SIZE
        } else {
            e.max_len - offset
        };
        let sendnow = tsend.wrapping_add(offset as usize);
        if tosend <= 0 {
            self.usb_send_empty(sendtok);
        } else {
            unsafe { self.usb_send_data(sendnow, tosend, 0, sendtok) };
        }
    }

    extern "C" fn usb_pid_handle_data(
        &mut self,
        _this_token: u32,
        data: *mut u8,
        which_data: u32,
        mut length: u32,
    ) {
        let epno = self.current_endpoint;

        let e = &mut self.eps[epno as usize];

        length -= 3;

        // Already received this packet.
        if e.toggle_out != which_data {
            unsafe {
                self.usb_send_data(core::ptr::null(), 0, 2, 0xD2); // Send ACK
            }
            return;
        }
        e.toggle_out = e.toggle_out ^ 0b1;

        if epno != 0 || ((self.setup_request == 0) && length > 3) {
            if self.reboot_armed > 0 {
                let data_u32 = data as *const u32;
                if unsafe {
                    epno == 0
                        && data_u32.read_unaligned() == 0xaa3412fd
                        && (data_u32.add(1).read_unaligned() & 0x00ffffff) == 0x00ddccbb
                } {
                    e.count = 7;
                    self.reboot_armed = 2;
                } else {
                    self.reboot_armed = 0;
                }
            }
        } else if self.setup_request != 0 {
            let s = unsafe { &mut *(data as *mut UsbUrb) };
            let wvi = s.l_value_lsb_index_msb;
            let w_length = s.w_length;
            //Send just a data packet.
            e.count = 0;
            e.opaque = core::ptr::null_mut();
            e.custom = 0;
            e.max_len = 0;
            self.setup_request = 0;

            // We shift down because we don't care if USB_RECIP_INTERFACE is set or not.
            // Otherwise we have to write extra code to handle each case if it's set or
            // not set, but in general, there's never a situation where we really care.
            let req_shl = s.w_request_type_lsb_request_msb >> 1;
            if req_shl == (0x0921 >> 1) {
                // Class request (Will be writing)  This is hid_send_feature_report
                if wvi == 0x000003fd {
                    self.reboot_armed = 1;
                }
            } else if req_shl == (0x0680 >> 1) {
                let (descriptor_addr, descriptor_len) = (self.get_descriptor_info)(wvi);
                e.opaque = descriptor_addr;
                let sw_len = w_length as u32;
                let el_len = descriptor_len as u32;
                e.max_len = if sw_len < el_len { sw_len } else { el_len };
            } else if req_shl == (0x0500 >> 1) {
                // SET_ADDRESS = 0x05
                self.my_address = wvi;
            }
        }
        // Got the right data. Acknowledge.
        unsafe { self.usb_send_data(core::ptr::null_mut(), 0, 2, 0xD2) }; // Send ACK
    }

    unsafe extern "C" fn usb_pid_handle_ack(&mut self, _dummy: u32, _data: *mut u8) {
        self.eps
            .get_unchecked_mut(self.current_endpoint as usize)
            .toggle_in ^= 1;
        self.eps
            .get_unchecked_mut(self.current_endpoint as usize)
            .count += 1;
    }

    unsafe extern "C" fn usb_pid_handle_setup(&mut self, _addr: u32, _data: *mut u8, endp: u32) {
        self.current_endpoint = endp;
        self.setup_request = 1;
        unsafe {
            self.eps.get_unchecked_mut(endp as usize).toggle_in = 1;
            self.eps.get_unchecked_mut(endp as usize).count = 0;
            self.eps.get_unchecked_mut(endp as usize).opaque = core::ptr::null();
            self.eps.get_unchecked_mut(endp as usize).toggle_out = 0;
        }
    }

    unsafe extern "C" fn usb_pid_handle_out(&mut self, _addr: u32, _data: *mut u8, endp: u32) {
        self.current_endpoint = endp;
    }
}
