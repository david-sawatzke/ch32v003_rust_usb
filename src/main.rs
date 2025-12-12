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

mod usb;
use usb::{
    rv003usb_internal, rv003usb_internal_data, usb_endpoint, usb_send_data, usb_send_empty, UsbIf,
};

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
    let mut _usb_dp = Input::new(p.PC3, Pull::None);
    let pin_number = p.PC2.pin() as usize;
    let port_number = p.PC2.port();
    let mut _usb_dm = Input::new(p.PC2, Pull::None);
    let mut usb_dpu = Output::new(p.PC5, Level::Low, Speed::High);
    // This is GPIOD, but i haven't figured out how to do this nicely yet
    let mut usb: UsbIf<0x4001_1000usize, 3, 2> = UsbIf {};
    unsafe {
        rv003usb_internal_data.se0_windup = 0;
    }
    // Do this here to force the rust compile to build them
    unsafe {
        usb.make_funcs();
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
    _e: *mut usb_endpoint,
    _scratchpad: *mut u8,
    endp: i32,
    sendtok: u32,
    _ist: *const rv003usb_internal,
) {
    if endp == 1 {
        // Mouse (4 bytes)
        unsafe {
            I_MOUSE += 1;
            let mode = I_MOUSE >> 5;

            TSAJOYSTICK_MOUSE[1] = 0;
            TSAJOYSTICK_MOUSE[2] = 0;
            // Move the mouse right, down, left and up in a square.
            // match mode & 3 {
            //     0 => {
            //         TSAJOYSTICK_MOUSE[1] = 1;
            //         TSAJOYSTICK_MOUSE[2] = 0;
            //     }
            //     1 => {
            //         TSAJOYSTICK_MOUSE[1] = 0;
            //         TSAJOYSTICK_MOUSE[2] = 1;
            //     }
            //     2 => {
            //         TSAJOYSTICK_MOUSE[1] = -1i8 as u8; // Need to cast to u8 for the array
            //         TSAJOYSTICK_MOUSE[2] = 0;
            //     }
            //     3 => {
            //         TSAJOYSTICK_MOUSE[1] = 0;
            //         TSAJOYSTICK_MOUSE[2] = -1i8 as u8; // Need to cast to u8 for the array
            //     }
            //     _ => {}
            // }
            usb_send_data(TSAJOYSTICK_MOUSE.as_ptr(), 4, 0, sendtok);
        }
    } else if endp == 2 {
        // Keyboard (8 bytes)
        unsafe {
            usb_send_data(TSAJOYSTICK_KEYBOARD.as_ptr(), 8, 0, sendtok);

            //I_KEYBOARD += 1;

            // Press a Key every second or so.
            if (I_KEYBOARD & 0x7f) == 1 {
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
