#include <stdint.h>

#define INSTANCE_DESCRIPTORS 1

#include "rv003usb.h"
#include "usb_config.h"

#if !defined(RV003USB_CUSTOM_C) || RV003USB_CUSTOM_C == 0


extern struct rv003usb_internal rv003usb_internal_data;


void usb_pid_handle_data( uint32_t this_token, uint8_t * data, uint32_t which_data, uint32_t length, struct rv003usb_internal * ist )
{
	//Received data from host.
	int epno = ist->current_endpoint;
	struct usb_endpoint * e = &ist->eps[epno];

	length -= 3;
	uint8_t * data_in = __builtin_assume_aligned( data, 4 );

	// Already received this packet.
	if( e->toggle_out != which_data )
	{
		goto just_ack;
	}

	e->toggle_out = !e->toggle_out;


#if RV003USB_HANDLE_USER_DATA || RV003USB_USE_REBOOT_FEATURE_REPORT || RV003USB_USB_TERMINAL
	if( epno || ( !ist->setup_request && length > 3 )  )
	{

#if RV003USB_HANDLE_USER_DATA
		usb_handle_user_data( e, epno, data_in, length, ist );
#endif
#if RV003USB_USER_DATA_HANDLES_TOKEN
		return;
#endif
	}
	else
#endif

	if( ist->setup_request )
	{
		// For receiving CONTROL-OUT messages into RAM.  NOTE: MUST be ALIGNED to 4, and be allocated round_up( payload, 8 ) + 4
		// opaque points to [length received, but uninitialized before complete][data...]
#if RV003USB_SUPPORT_CONTROL_OUT
		if( ist->setup_request == 2 )
		{
			// This mode is where we record control-in data into a pointer and mark it as 
			int offset = e->count << 3;
			uint32_t * base = __builtin_assume_aligned( e->opaque, 4 );
			uint32_t * dout = __builtin_assume_aligned( ((uint8_t*)base) + offset + 4, 4 );
			uint32_t * din = __builtin_assume_aligned( data_in, 4 );
			if( offset < e->max_len )
			{
				dout[0] = din[0];
				dout[1] = din[1];
				e->count++;
				if( offset + 8 >= e->max_len )
				{
					base[0] = e->max_len;
				}
			}
			goto just_ack;
		}
#endif

		struct usb_urb * s = __builtin_assume_aligned( (struct usb_urb *)(data_in), 4 );

		uint32_t wvi = s->lValueLSBIndexMSB;
		uint32_t wLength = s->wLength;
		//Send just a data packet.
		e->count = 0;
		e->opaque = 0;
		e->custom = 0;
		e->max_len = 0;
		ist->setup_request = 0;

		//int bRequest = s->wRequestTypeLSBRequestMSB >> 8;

		// We shift down because we don't care if USB_RECIP_INTERFACE is set or not.
		// Otherwise we have to write extra code to handle each case if it's set or
		// not set, but in general, there's never a situation where we really care.
		uint32_t reqShl = s->wRequestTypeLSBRequestMSB >> 1;

		//LogUEvent( 0, s->wRequestTypeLSBRequestMSB, wvi, s->wLength );

		if( reqShl == (0x0921>>1) )
		{
			// Class request (Will be writing)  This is hid_send_feature_report
#if RV003USB_HID_FEATURES
			usb_handle_hid_set_report_start( e, wLength, wvi );
#endif
		}
		else
#if RV003USB_HID_FEATURES || RV003USB_USB_TERMINAL
		if( reqShl == (0x01a1>>1) )
		{
			// Class read request.
			// The host wants to read back from us. hid_get_feature_report
#if RV003USB_HID_FEATURES
			usb_handle_hid_get_report_start( e, wLength, wvi );
#endif
		}
		else
#endif
		if( reqShl == (0x0680>>1) ) // GET_DESCRIPTOR = 6 (msb)
		{
			int i;
			const struct descriptor_list_struct * dl;
			for( i = 0; i < DESCRIPTOR_LIST_ENTRIES; i++ )
			{
				dl = &descriptor_list[i];
				if( dl->lIndexValue == wvi )
				{
					e->opaque = (uint8_t*)dl->addr;
					uint16_t swLen = wLength;
					uint16_t elLen = dl->length;
					e->max_len = (swLen < elLen)?swLen:elLen;
				}
			}
#if RV003USB_EVENT_DEBUGGING
			if( !e->max_len ) LogUEvent( 1234, dl->lIndexValue, dl->length, 0 );
#endif
		}
		else if( reqShl == (0x0500>>1) ) // SET_ADDRESS = 0x05
		{
			ist->my_address = wvi;
		}
		//  You could handle SET_CONFIGURATION == 0x0900 here if you wanted.
		//  Can also handle GET_CONFIGURATION == 0x0880 to which we send back { 0x00 }, or the interface number.  (But no one does this).
		//  You could handle SET_INTERFACE == 0x1101 here if you wanted.
		//   or
		//  USB_REQ_GET_INTERFACE to which we would send 0x00, or the interface #
		else
		{
#if RV003USB_OTHER_CONTROL
			usb_handle_other_control_message( e, s, ist );
#endif
		}
	}
just_ack:
	{
		//Got the right data.  Acknowledge.
		usb_send_data( 0, 0, 2, 0xD2 ); // Send ACK
	}
	return;
}
#endif
