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

const ENDP_OFFSET: u8 = 28;
const EP_COUNT_OFFSET: u8 = 0;
const EP_TOGGLE_IN_OFFSET: u8 = 4;

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
            ENDP_OFFSET = const ENDP_OFFSET,
            EP_TOGGLE_IN_OFFSET = const EP_TOGGLE_IN_OFFSET,
            EP_COUNT_OFFSET = const EP_COUNT_OFFSET,
    );
}
