use usbd_hid::descriptor::generator_prelude::*;
use utf16_lit::utf16;

// --- 2. HID Report Descriptors (The "Smart" Way) ---

#[gen_hid_descriptor(
    (collection = APPLICATION, usage_page = GENERIC_DESKTOP, usage = MOUSE) = {
        (collection = PHYSICAL, usage = POINTER) = {
            (usage_page = BUTTON, usage_min = BUTTON_1, usage_max = BUTTON_3) = {
                #[packed_bits 3] #[item_settings data,variable,absolute] buttons=input;
            };
            (usage_page = GENERIC_DESKTOP,) = {
                (usage = X,) = {#[item_settings data,variable,relative] x=input};
                (usage = Y,) = {#[item_settings data,variable,relative] y=input};
                (usage = WHEEL,) = {#[item_settings data,variable,relative] wheel=input};
            }
        }
    }
)]
struct MouseReport {
    buttons: u8,
    x: i8,
    y: i8,
    wheel: i8,
}
// We need this value for the CONFIG_DESCRIPTOR, unfortunately there is no way
// to get the hid descriptor statically. Thus hardcode it here and verify that it matches later on.
// How do you get the real length without guessing? I just made a separate project that printed it
const MOUSE_DESC_LEN: usize = 57;

/// KeyboardReport describes a report and its companion descriptor that can be
/// used to send keyboard button presses to a host and receive the status of the
/// keyboard LEDs.
#[gen_hid_descriptor(
    (collection = APPLICATION, usage_page = GENERIC_DESKTOP, usage = KEYBOARD) = {
        (usage_page = KEYBOARD, usage_min = 0xE0, usage_max = 0xE7) = {
            #[packed_bits 8] #[item_settings data,variable,absolute] modifier=input;
        };
        (usage_min = 0x00, usage_max = 0xFF) = {
            #[item_settings constant,variable,absolute] reserved=input;
        };
        (usage_page = LEDS, usage_min = 0x01, usage_max = 0x05) = {
            #[packed_bits 5] #[item_settings data,variable,absolute] leds=output;
        };
        (usage_page = KEYBOARD, usage_min = 0x00, usage_max = 0xDD) = {
            #[item_settings data,array,absolute] keycodes=input;
        };
    }
)]
#[allow(dead_code)]
pub struct KeyboardReport {
    pub modifier: u8,
    pub reserved: u8,
    pub leds: u8,
    pub keycodes: [u8; 6],
}
const KBD_DESC_LEN: usize = 69;

#[link_section = ".rodata"]
static DEVICE_DESCRIPTOR: [u8; 18] = [
    18, // Length
    1,  // Type (Device)
    0x10, 0x01, // Spec
    0x0,  // Device Class
    0x0,  // Device Subclass
    0x0,  // Device Protocol  (000 = use config descriptor)
    0x08, // Max packet size for EP0 (This has to be 8 because of the USB
    // Low-Speed Standard)
    0x09, 0x12, // ID Vendor
    0x03, 0xc0, // ID Product
    0x02, 0x00, // ID Rev
    1,    // Manufacturer string
    2,    // Product string
    3,    // Serial string
    1,    // Max number of configurations
];

#[link_section = ".rodata"]
static CONFIG_DESCRIPTOR: [u8; 59] = [
    // Mostly stolen from a USB mouse I found.
    // configuration descriptor, USB spec 9.6.3, page 264-266, Table 9-10
    9, // bLength;
    2, // bDescriptorType;
    0x3b,
    0x00, // wTotalLength
    0x02, // bNumInterfaces (Normally 1)
    0x01, // bConfigurationValue
    0x00, // iConfiguration
    0x80, // bmAttributes (was 0xa0)
    0x64, // bMaxPower (200mA)
    // This descriptor shows how to embed two HIDs, to build a composite HID
    // device.

    // Mouse
    9,    // bLength
    4,    // bDescriptorType
    0,    // bInterfaceNumber (unused, would normally be used for HID)
    0,    // bAlternateSetting
    1,    // bNumEndpoints
    0x03, // bInterfaceClass (0x03 = HID)
    0x01, // bInterfaceSubClass
    0x02, // bInterfaceProtocol (Mouse)
    0,    // iInterface
    9,    // bLength
    0x21, // bDescriptorType (HID)
    0x10,
    0x01, // bcd 1.1
    0x00, // country code
    0x01, // Num descriptors
    0x22, // DescriptorType[0] (HID)
    MOUSE_DESC_LEN as u8,
    0x00,
    7,    // endpoint descriptor (For endpoint 1)
    0x05, // Endpoint Descriptor (Must be 5)
    0x81, // Endpoint Address
    0x03, // Attributes
    0x04,
    0x00, // Size
    10,   // Interval (Number of milliseconds between polls)
    // Keyboard  (It is unusual that this would be here)
    9,    // bLength
    4,    // bDescriptorType
    1,    // bInterfaceNumber  = 1 instead of 0 -- well make it second.
    0,    // bAlternateSetting
    1,    // bNumEndpoints
    0x03, // bInterfaceClass (0x03 = HID)
    0x01, // bInterfaceSubClass
    0x01, // bInterfaceProtocol (??)
    0,    // iInterface
    9,    // bLength
    0x21, // bDescriptorType (HID)
    0x10,
    0x01, // bcd 1.1
    0x00, // country code
    0x01, // Num descriptors
    0x22, // DescriptorType[0] (HID)
    KBD_DESC_LEN as u8,
    0x00,
    7,    // endpoint descriptor (For endpoint 1)
    0x05, // Endpoint Descriptor (Must be 5)
    0x82, // Endpoint Address
    0x03, // Attributes
    0x08,
    0x00, // Size (8 bytes)
    10,   // Interval Number of milliseconds between polls.
];

// --- 3. String Descriptors ---

// Rust strings are UTF-8, USB requires UTF-16LE.
// A simple helper struct to mimic the C memory layout.
#[repr(C, packed)]
struct UsbStringDesc<const N: usize> {
    b_length: u8,
    b_descriptor_type: u8,
    w_string: [u16; N],
}

// Helper to create the struct
const fn make_string<const N: usize>(s: &[u16; N]) -> UsbStringDesc<N> {
    UsbStringDesc {
        b_length: (2 * N + 2) as u8,
        b_descriptor_type: 3, // STRING type
        w_string: *s,
    }
}

// Define your strings manually as UTF-16 arrays
static STR_LANG: UsbStringDesc<1> = make_string(&[0x0409]); // English
static STR_MANUF: UsbStringDesc<6> = make_string(&utf16!("CNLohr"));
static STR_PROD: UsbStringDesc<8> = make_string(&utf16!("RV003USB"));
static STR_SERIAL: UsbStringDesc<3> = make_string(&utf16!("000"));
static STR_ERR: UsbStringDesc<3> = make_string(&utf16!("ERR"));

// --- 4. The Lookup Logic ---

/// This replaces your 'descriptor_list' and loop.
/// Returns a tuple: (Pointer to data, Length of data)
#[no_mangle]
pub extern "C" fn get_descriptor_info(w_value: u32) -> (*const u8, u16) {
    let slice: &[u8] = match w_value {
        0x00000100 => &DEVICE_DESCRIPTOR,
        0x00000200 => &CONFIG_DESCRIPTOR,
        // For HID Report (0x22), we use the generated desc from the struct above
        0x00002200 => MouseReport::desc(),
        0x00012200 => KeyboardReport::desc(),
        0x00000300 => unsafe {
            // Cast struct to u8 slice for transmission
            core::slice::from_raw_parts(
                &STR_LANG as *const _ as *const u8,
                STR_LANG.b_length as usize,
            )
        },
        0x04090301 => unsafe {
            core::slice::from_raw_parts(
                &STR_MANUF as *const _ as *const u8,
                STR_MANUF.b_length as usize,
            )
        },
        0x04090302 => unsafe {
            if MouseReport::desc().len() != MOUSE_DESC_LEN as usize
                || KeyboardReport::desc().len() != KBD_DESC_LEN as usize
            {
                core::slice::from_raw_parts(
                    &STR_ERR as *const _ as *const u8,
                    STR_ERR.b_length as usize,
                )
            } else {
                core::slice::from_raw_parts(
                    &STR_PROD as *const _ as *const u8,
                    STR_PROD.b_length as usize,
                )
            }
        },
        0x04090303 => unsafe {
            core::slice::from_raw_parts(
                &STR_SERIAL as *const _ as *const u8,
                STR_SERIAL.b_length as usize,
            )
        },
        _ => &[], // Return empty slice for null/not found
    };

    (slice.as_ptr(), slice.len() as u16)
}

// TODO find a way to run this test
#[test]
fn check_descriptors() {
    assert_eq!(MouseReport::desc().len(), MOUSE_DESC_LEN as usize);
    assert_eq!(KeyboardReport::desc().len(), KBD_DESC_LEN as usize);
}
