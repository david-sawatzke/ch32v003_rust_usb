use crate::descriptors;
use crate::usb_handle_user_in_request;
use core::mem;

#[no_mangle]
#[used]
pub static mut rv003usb_internal_data: rv003usb_internal = unsafe { mem::zeroed() };
extern "C" {
    pub fn usb_send_data(data: *const u8, length: u32, poly_function: u32, token: u32);
    pub fn usb_send_empty(token: u32);
}
const ENDPOINT0_SIZE: u32 = 8;
// Assumes RV003USB_OPTIMIZE_FLASH
#[repr(C)]
pub struct usb_endpoint {
    count: u32,
    toggle_in: u32,
    toggle_out: u32,
    custom: u32,
    max_len: u32,
    reserved1: u32,
    reserved2: u32,
    opaque: *mut u8,
}
#[repr(C)]
pub struct rv003usb_internal {
    current_endpoint: u32,
    my_address: u32,
    setup_request: u32,
    reserved: u32,
    last_se0_cyccount: u32,
    delta_se0_cyccount: i32,
    pub se0_windup: u32,
    eps: [usb_endpoint; 3], // ENDPOINTS
}
#[repr(C, packed)]
pub struct usb_urb {
    wRequestTypeLSBRequestMSB: u16,
    lValueLSBIndexMSB: u32,
    wLength: u16,
}

#[no_mangle]
pub extern "C" fn usb_pid_handle_data(
    this_token: u32,
    data: *mut u8,
    which_data: u32,
    length: u32,
    ist: *mut rv003usb_internal,
) {
    let epno = unsafe { (*ist).current_endpoint };

    let e = unsafe { &mut (*ist).eps[epno as usize] };
    let length = length - 3;
    let data_in = data;

    // Already received this packet.
    if e.toggle_out != which_data {
        unsafe {
            usb_send_data(core::ptr::null(), 0, 2, 0xD2); // Send ACK
        }
        return;
    }
    e.toggle_out = !e.toggle_out;

    if unsafe { (*ist).setup_request } != 0 {
        let s = unsafe { &mut *(data as *mut usb_urb) };
        let wvi = s.lValueLSBIndexMSB;
        let wLength = s.wLength;
        //Send just a data packet.
        e.count = 0;
        e.opaque = core::ptr::null_mut();
        e.custom = 0;
        e.max_len = 0;
        unsafe { (*ist).setup_request = 0 };

        // We shift down because we don't care if USB_RECIP_INTERFACE is set or not.
        // Otherwise we have to write extra code to handle each case if it's set or
        // not set, but in general, there's never a situation where we really care.
        let reqShl = s.wRequestTypeLSBRequestMSB >> 1;
        if reqShl == (0x0921 >> 1) {
            // Class request (Will be writing)  This is hid_send_feature_report
        } else if reqShl == (0x0680 >> 1) {
            let (descriptor_addr, descriptor_len) = descriptors::get_descriptor_info(wvi);
            e.opaque = descriptor_addr as *mut u8;
            let sw_len = wLength as u32;
            let el_len = descriptor_len as u32;
            e.max_len = if sw_len < el_len { sw_len } else { el_len };
        } else if reqShl == (0x0500 >> 1) {
            // SET_ADDRESS = 0x05
            unsafe { (*ist).my_address = wvi };
        }
    }
    // Got the right data. Acknowledge.
    unsafe { usb_send_data(core::ptr::null_mut(), 0, 2, 0xD2) }; // Send ACK
}
#[no_mangle]
pub extern "C" fn usb_pid_handle_in(
    addr: u32,
    data: *mut u8,
    endp: u32,
    _unused: u32,
    ist: *mut rv003usb_internal,
) {
    unsafe { (*ist).current_endpoint = endp };

    let e = unsafe { &mut (*ist).eps[endp as usize] };
    let sendtok = if e.toggle_in != 0 {
        0b01001011
    } else {
        0b11000011
    };
    // if RV003USB_HANDLE_IN_REQUEST
    if (e.custom != 0) || (endp != 0) {
        usb_handle_user_in_request(e, data, endp as i32, sendtok, ist);
        return;
    }
    // endif
    let tsend = e.opaque;
    let offset = e.count << 3;
    let tosend = if (e.max_len - offset) > ENDPOINT0_SIZE {
        ENDPOINT0_SIZE
    } else {
        e.max_len - offset
    };
    let sendnow = tsend.wrapping_add(offset as usize);
    if tosend <= 0 {
        unsafe { usb_send_empty(sendtok) };
    } else {
        unsafe { usb_send_data(sendnow, tosend, 0, sendtok) };
    }
}

#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn usb_pid_handle_ack(
    dummy: u32,
    data: *mut u8,
    dummy1: u32,
    dummy2: u32,
    ist: *mut rv003usb_internal,
) {
    core::arch::naked_asm!(
        "c.lw a2, 0(a4) //ist->current_endpoint -> endp;",
        "c.slli a2, 5",
        "c.add a2, a4",
        "c.addi a2, {ENDP_OFFSET} // usb_endpoint eps[ENDPOINTS];",
        "c.lw a0, ({EP_TOGGLE_IN_OFFSET})(a2) // toggle_in=!toggle_in",
        "c.li a1, 1",
        "c.xor a0, a1",
        "c.sw a0, ({EP_TOGGLE_IN_OFFSET})(a2)",
        "c.lw a0, ({EP_COUNT_OFFSET})(a2) // count_in",
        "c.addi a0, 1",
        "c.sw a0, ({EP_COUNT_OFFSET})(a2)",
        "c.j done_usb_message_in",
            ENDP_OFFSET = const mem::offset_of!(rv003usb_internal, eps),
            EP_TOGGLE_IN_OFFSET = const mem::offset_of!(usb_endpoint, toggle_in),
            EP_COUNT_OFFSET = const mem::offset_of!(usb_endpoint, count),
    );
}

#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn usb_pid_handle_setup(
    addr: u32,
    data: *mut u8,
    endp: u32,
    unused: u32,
    ist: *mut rv003usb_internal,
) {
    core::arch::naked_asm!(
    "c.sw a2, 0(a4) // ist->current_endpoint = endp",
    "c.li a1, 1",
        "c.sw a1, {SETUP_REQUEST_OFFSET}(a4) //ist->setup_request = 1;",
    "c.slli a2, 3+2",
    "c.add a2, a4",
        "c.sw a1, ({ENDP_OFFSET}+{EP_TOGGLE_IN_OFFSET})(a2) //e->toggle_in = 1;",
    "c.li a1, 0",
        "c.sw a1, ({ENDP_OFFSET}+{EP_COUNT_OFFSET})(a2)  //e->count = 0;",
        "c.sw a1, ({ENDP_OFFSET}+{EP_OPAQUE_OFFSET})(a2)  //e->opaque = 0;",
        "c.sw a1, ({ENDP_OFFSET}+{EP_TOGGLE_OUT_OFFSET})(a2) //e->toggle_out = 0;",
    "c.j done_usb_message_in",
    ENDP_OFFSET = const mem::offset_of!(rv003usb_internal, eps),
    SETUP_REQUEST_OFFSET = const mem::offset_of!(rv003usb_internal, setup_request),
    EP_COUNT_OFFSET = const mem::offset_of!(usb_endpoint, count),
    EP_TOGGLE_IN_OFFSET = const mem::offset_of!(usb_endpoint, toggle_in),
    EP_TOGGLE_OUT_OFFSET = const mem::offset_of!(usb_endpoint, toggle_out),
    EP_OPAQUE_OFFSET = const mem::offset_of!(usb_endpoint, opaque),
    );
}

#[unsafe(naked)]
#[no_mangle]
pub unsafe extern "C" fn usb_pid_handle_out(
    addr: u32,
    data: *mut u8,
    endp: u32,
    unused: u32,
    ist: *mut rv003usb_internal,
) {
    core::arch::naked_asm!(
        "c.sw a2, 0(a4) // ist->current_endpoint = endp",
        "c.j done_usb_message_in"
    );
}

pub struct UsbIf<const USB_BASE: usize, const DP: u8, const DM: u8> {}
impl<const USB_BASE: usize, const DP: u8, const DM: u8> UsbIf<USB_BASE, DP, DM> {
    // Advanced "pfusch" to force rust to make the generic and keep it until linker phase
    // Hopefully can be dropped in the future
    // I'm unsure if we can drop this entirely in the future
    // Probably not for the interrupt handler?
    pub(crate) fn make_funcs(&mut self) {
        unsafe { core::arch::asm!("// {}", sym Self::handle_se0_keepalive) }
        unsafe { core::arch::asm!("// {}", sym Self::usb_send_empty) }
    }
    //#[unsafe(naked)]
    //#[unsafe(link_section = ".text.vector_handler")]
    //#[no_mangle]
    //pub fn EXTI7_0_IRQHandler1() {
    //core::arch::naked_asm!();
    //}
    #[allow(no_mangle_generic_items)]
    #[no_mangle]
    #[allow(named_asm_labels)]
    #[unsafe(naked)]
    pub(crate) unsafe extern "C" fn handle_se0_keepalive() {
        core::arch::naked_asm!(
            // Needed ...
            ".global handle_se0_keepalive",
            // In here, we want to do smart stuff with the
            // 1ms tick.
            "la  a0, 0xE000F008", // SYSTICK_CNT
            "la a4, rv003usb_internal_data",
            "c.lw a1, {LAST_SE0_OFFSET}(a4) //last cycle count   last_se0_cyccount",
            "c.lw a2, 0(a0) //this cycle coun",
            "c.sw a2, {LAST_SE0_OFFSET}(a4) //store it back to last_se0_cyccount",
            "c.sub a2, a1",
            "c.sw a2, {DELTA_SE0_OFFSET}(a4) //record delta_se0_cyccount",
            "li a1, 48000",
            "c.sub a2, a1",
            // This is our deviance from 48MHz.

            // Make sure we aren't in left field.
            "li a5, 4000",
            "bge a2, a5, ret_from_se0",
            "li a5, -4000",
            "blt a2, a5, ret_from_se0",
            "c.lw a1, {SE0_WINDUP_OFFSET}(a4) // load windup se0_windup",
            "c.add a1, a2",
            "c.sw a1, {SE0_WINDUP_OFFSET}(a4) // save windup",
            // No further adjustments
            "beqz a1, ret_from_se0",
            // 0x40021000 = RCC.CTLR
            "la a4, 0x40021000",
            "lw a0, 0(a4)",
            "srli a2, a0, 3 // Extract HSI Trim.",
            "andi a2, a2, 0b11111",
            "li a5, 0xffffff07",
            "and a0, a0, a5 // Mask off non-HSI",
            // Decimate windup - use as HSIrim.
            "neg a1, a1
            srai a2, a1, 9",
            "addi a2, a2, 16  // add HSI offset.",
            // Put trim in place in register.
            "slli a2, a2, 3",
            "or a0, a0, a2",
            "sw a0, 0(a4)",
            "j ret_from_se0",
            SE0_WINDUP_OFFSET = const mem::offset_of!(rv003usb_internal, se0_windup),
            LAST_SE0_OFFSET = const mem::offset_of!(rv003usb_internal, last_se0_cyccount),
            DELTA_SE0_OFFSET = const mem::offset_of!(rv003usb_internal, delta_se0_cyccount),
        );
        // TODO get periph register addresses from/to proper addr
    }
    #[allow(no_mangle_generic_items)]
    #[no_mangle]
    #[allow(named_asm_labels)]
    #[unsafe(naked)]
    pub(crate) unsafe extern "C" fn usb_send_empty() {
        // Also contains usb_send_data
        core::arch::naked_asm!(
            ".macro nx6p3delay_usb_send n, freereg",
            "li \\freereg, ((\\n) + 1)",
            "1: c.addi \\freereg, -1",
            "c.bnez \\freereg, 1b",
            ".endm",
            // Needed ...
            ".global usb_send_empty",
            ".global usb_send_data",
            ".balign 4",
            //void usb_send_empty( uint32_t token );
            "c.mv a3, a0",
            "la a0, always0",
            "li a1, 2",
            "c.mv a2, a1",
            //void usb_send_data( uint8_t * data, uint32_t length, uint32_t poly_function, uint32_t token );
            "usb_send_data:",
            "addi	sp,sp,-16",
            "sw	s0, 0(sp)",
            "sw	s1, 4(sp)",

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

            "li t1, (1<<{USB_PIN_DP}) | (1<<({USB_PIN_DM}+16)) | (1<<{USB_PIN_DM}) | (1<<({USB_PIN_DP}+16));",

            //"SAVE_DEBUG_MARKER( 8 )

            // Save off our preamble and token.
            "c.slli a3, 7    ", //Put token further up so it gets sent later.
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
            //"DEBUG_TICK_SETUP",

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
            "nx6p3delay_usb_send 2, a3 ;	c.nop;            ", // Free time!
            "c.j pre_and_tok_send_inner_loop",
            ".balign 4",
            "pre_and_tok_done_sending_data:",
            ////////////////////////////////////////////////////////////////////////////

            // We have very little time here.  Just enough to do this.

            //Restore size.
            "mv a1, t2//lw  a1, 12(sp)",
            "c.beqz a1, no_really_done_sending_data ", //No actual payload?  Bail!
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
            "c.srai a3,31   ", // Copy MSB into all other bits

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
            "beqz t0, send_inner_loop ", //Second CRC byte
            // First CRC byte
            "not s0, a2", // get read to send out the CRC.
            "c.j send_inner_loop",


            ".balign 4",
            "no_really_done_sending_data:",

            //	c.bnez a2, poly_function  TODO: Uncomment me!

            "nx6p3delay_usb_send 2, a3 ;",

            // Need to perform an SE0.
            "li s1, (1<<({USB_PIN_DM}+16)) | (1<<({USB_PIN_DP}+16))",
            "c.sw s1, {BSHR_OFFSET}(a5)",

            "nx6p3delay_usb_send 7, a3 ;",

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
            //"RESTORE_DEBUG_MARKER( 8 )",
            "addi	sp,sp,16",
            "ret",

            ".balign 4",
            // TODO: This seems to be either 222 or 226 (not 224) in cases.
            // It's off by 2 clock cycles.  Probably OK, but, hmm.
            "insert_stuffed_bit:",
            "nx6p3delay_usb_send 3, a3",
            "xor s1, s1, t1",
            "c.li a4, 6", // reset bit stuffing.
            "c.nop",
            "c.nop",
            "sw s1, {BSHR_OFFSET}(a5)",
            "c.j send_end_bit_complete",
            ".balign 4",
            "always0:",
            ".word 0x00",
            USB_GPIO_BASE            = const USB_BASE,
            USB_PIN_DP            = const DP,
            USB_PIN_DM            = const DM,
            BSHR_OFFSET            = const 16, // Don't see a good way to get this from the pac?
            CFGLR_OFFSET            = const 0,
        );
    }
}
