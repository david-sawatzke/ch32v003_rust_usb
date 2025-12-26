#ifndef _USB_CONFIG_H
#define _USB_CONFIG_H

// Defines the number of endpoints for this device. (Always add one for EP0).
// For two EPs, this should be 3.
#define ENDPOINTS 3

#define USB_PORT C    // [A,C,D] GPIO Port to use with D+, D- and DPU
#define USB_PIN_DP 3  // [0-4] GPIO Number for USB D+ Pin
#define USB_PIN_DM 2  // [0-4] GPIO Number for USB D- Pin
#define USB_PIN_DPU 5 // [0-7] GPIO for feeding the 1.5k Pull-Up on USB D-
// Pin; Comment out if not used / tied to 3V3!

#define RV003USB_OPTIMIZE_FLASH 1
#define RV003USB_EVENT_DEBUGGING 0
#define RV003USB_HANDLE_IN_REQUEST 1
#define RV003USB_OTHER_CONTROL 0
#define RV003USB_HANDLE_USER_DATA 0
#define RV003USB_HID_FEATURES 0

#endif
