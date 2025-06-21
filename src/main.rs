#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(impl_trait_in_assoc_type)]
use qingke::riscv::interrupt::machine;

use hal::delay::Delay;
use hal::gpio::{Input, Level, Output, Pin, Pull, Speed};
use hal::interrupt::Interrupt;
use hal::pac;
use {ch32_hal as hal, panic_halt as _};
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
    se0_windup: u32,
    eps: [usb_endpoint; 3], // ENDPOINTS
}

extern "C" {
    static rv003usb_internal_data: *mut rv003usb_internal;
    pub fn usb_setup();
    pub fn usb_send_data(data: *const u8, length: u32, poly_function: u32, token: u32);
    pub fn usb_send_empty(token: u32);

}
const ENDPOINT0_SIZE: u32 = 8;

#[qingke_rt::entry]
fn main() -> ! {
    // hal::debug::SDIPrint::enable();
    let mut config = hal::Config::default();
    config.rcc = hal::rcc::Config::SYSCLK_FREQ_48MHZ_HSI;
    let p = hal::init(config);

    let mut delay = Delay;

    let mut led1 = Output::new(p.PA1, Level::Low, Default::default());
    let mut led2 = Output::new(p.PC0, Level::Low, Default::default());

    // USB setup
    let mut _usb_dp = Input::new(p.PD4, Pull::None);
    let pin_number = p.PD3.pin() as usize;
    let port_number = p.PD3.port();
    let mut _usb_dm = Input::new(p.PD3, Pull::None);
    let mut usb_dpu = Output::new(p.PD5, Level::Low, Speed::High);
    unsafe {
        (*rv003usb_internal_data).se0_windup = 0;
    }

    let exti = &pac::EXTI;
    let afio = &pac::AFIO;
    afio.exticr()
        .modify(|w| w.set_exti(pin_number, port_number));
    //Warning: The interrupts perform HSI trimming and should run with 48MHz HSI settings
    exti.intenr().modify(|w| w.set_mr(pin_number, true)); // enable interrupt
    exti.ftenr().modify(|w| w.set_tr(pin_number, true));
    exti.rtenr().modify(|w| w.set_tr(pin_number, false));

    usb_dpu.set_high();
    // EXTI7_0 is already enabled by the hal
    // USB setup done

    loop {
        led1.toggle();
        delay.delay_ms(1000);
        // hal::println!("toggle!");
        // let val = hal::pac::SYSTICK.cnt().read();
        // hal::println!("systick: {}", val);
    }
}

static mut I_MOUSE: i32 = 0;
static mut TSAJOYSTICK_MOUSE: [u8; 4] = [0x00, 0x00, 0x00, 0x00];
static mut I_KEYBOARD: i32 = 0;
static mut TSAJOYSTICK_KEYBOARD: [u8; 8] = [0x00; 8];

#[no_mangle]
pub extern "C" fn usb_handle_user_in_request(
    e: *mut usb_endpoint,
    scratchpad: *mut u8,
    endp: i32,
    sendtok: u32,
    ist: *const rv003usb_internal,
) {
    if endp == 1 {
        // Mouse (4 bytes)
        unsafe {
            I_MOUSE += 1;
            let mode = I_MOUSE >> 5;

            // Move the mouse right, down, left and up in a square.
            match mode & 3 {
                0 => {
                    TSAJOYSTICK_MOUSE[1] = 1;
                    TSAJOYSTICK_MOUSE[2] = 0;
                }
                1 => {
                    TSAJOYSTICK_MOUSE[1] = 0;
                    TSAJOYSTICK_MOUSE[2] = 1;
                }
                2 => {
                    TSAJOYSTICK_MOUSE[1] = -1i8 as u8; // Need to cast to u8 for the array
                    TSAJOYSTICK_MOUSE[2] = 0;
                }
                3 => {
                    TSAJOYSTICK_MOUSE[1] = 0;
                    TSAJOYSTICK_MOUSE[2] = -1i8 as u8; // Need to cast to u8 for the array
                }
                _ => {}
            }
            usb_send_data(TSAJOYSTICK_MOUSE.as_ptr(), 4, 0, sendtok);
        }
    } else if endp == 2 {
        // Keyboard (8 bytes)
        unsafe {
            usb_send_data(TSAJOYSTICK_KEYBOARD.as_ptr(), 8, 0, sendtok);

            I_KEYBOARD += 1;

            // Press a Key every second or so.
            if (I_KEYBOARD & 0x7f) == 0 {
                TSAJOYSTICK_KEYBOARD[4] = 0x05; // 0x05 = "b"; 0x53 = NUMLOCK; 0x39 = CAPSLOCK;
            } else {
                TSAJOYSTICK_KEYBOARD[4] = 0;
            }
        }
    } else {
        // If it's a control transfer, empty it.
        unsafe {
            usb_send_empty(sendtok);
        }
    }
}

#[no_mangle]
pub extern "C" fn usb_pid_handle_in(
    addr: u32,
    data: *mut u8,
    endp: u32,
    unused: u32,
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
    if (tosend <= 0) {
        unsafe { usb_send_empty(sendtok) };
    } else {
        unsafe { usb_send_data(sendnow, tosend, 0, sendtok) };
    }
}
mod _vectors {
    extern "C" {
        fn WWDG();
        fn PVD();
        fn FLASH();
        fn RCC();
        fn EXTI7_0_IRQHandler();
        fn AWU();
        fn DMA1_CHANNEL1();
        fn DMA1_CHANNEL2();
        fn DMA1_CHANNEL3();
        fn DMA1_CHANNEL4();
        fn DMA1_CHANNEL5();
        fn DMA1_CHANNEL6();
        fn DMA1_CHANNEL7();
        fn ADC();
        fn I2C1_EV();
        fn I2C1_ER();
        fn USART1();
        fn SPI1();
        fn TIM1_BRK();
        fn TIM1_UP();
        fn TIM1_TRG_COM();
        fn TIM1_CC();
        fn TIM2();
    }
    pub union Vector {
        _handler: unsafe extern "C" fn(),
        _reserved: u32,
    }
    #[link_section = ".vector_table.external_interrupts_usb"]
    #[no_mangle]
    pub static __EXTERNAL_INTERRUPTS_USB: [Vector; 23] = [
        Vector { _handler: WWDG },
        Vector { _handler: PVD },
        Vector { _handler: FLASH },
        Vector { _handler: RCC },
        Vector {
            _handler: EXTI7_0_IRQHandler,
        },
        Vector { _handler: AWU },
        Vector {
            _handler: DMA1_CHANNEL1,
        },
        Vector {
            _handler: DMA1_CHANNEL2,
        },
        Vector {
            _handler: DMA1_CHANNEL3,
        },
        Vector {
            _handler: DMA1_CHANNEL4,
        },
        Vector {
            _handler: DMA1_CHANNEL5,
        },
        Vector {
            _handler: DMA1_CHANNEL6,
        },
        Vector {
            _handler: DMA1_CHANNEL7,
        },
        Vector { _handler: ADC },
        Vector { _handler: I2C1_EV },
        Vector { _handler: I2C1_ER },
        Vector { _handler: USART1 },
        Vector { _handler: SPI1 },
        Vector { _handler: TIM1_BRK },
        Vector { _handler: TIM1_UP },
        Vector {
            _handler: TIM1_TRG_COM,
        },
        Vector { _handler: TIM1_CC },
        Vector { _handler: TIM2 },
    ];
}
