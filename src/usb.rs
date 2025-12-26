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
            "neg a1, a1",
            "srai a2, a1, 9",
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
}
