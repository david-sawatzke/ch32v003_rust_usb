#include <stdint.h>

#define INSTANCE_DESCRIPTORS 1

#include "rv003usb.h"
#include "usb_config.h"



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
		usb_send_data( 0, 0, 2, 0xD2 ); // Send ACK
		return;
	}

	e->toggle_out = !e->toggle_out;


	if( ist->setup_request )
	{
		struct usb_urb * s = __builtin_assume_aligned( (struct usb_urb *)(data_in), 4 );

		uint32_t wvi = s->lValueLSBIndexMSB;
		uint32_t wLength = s->wLength;
		//Send just a data packet.
		e->count = 0;
		e->opaque = 0;
		e->custom = 0;
		e->max_len = 0;
		ist->setup_request = 0;


		// We shift down because we don't care if USB_RECIP_INTERFACE is set or not.
		// Otherwise we have to write extra code to handle each case if it's set or
		// not set, but in general, there's never a situation where we really care.
		uint32_t reqShl = s->wRequestTypeLSBRequestMSB >> 1;


		if( reqShl == (0x0921>>1) )
		{
			// Class request (Will be writing)  This is hid_send_feature_report
		} else if( reqShl == (0x0680>>1) ) // GET_DESCRIPTOR = 6 (msb)
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
		}
		else if( reqShl == (0x0500>>1) ) // SET_ADDRESS = 0x05
		{
			ist->my_address = wvi;
		}
	}
	//Got the right data.  Acknowledge.
	usb_send_data( 0, 0, 2, 0xD2 ); // Send ACK
	return;
}
