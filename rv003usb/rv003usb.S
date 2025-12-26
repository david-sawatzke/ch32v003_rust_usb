#include "rv003usb.h"
// Taken from ch32v003fun

#define PERIPH_BASE                             (0x40000000) /* Peripheral base address in the alias region */
#define APB2PERIPH_BASE                         (PERIPH_BASE + 0x10000)
#define EXTI_BASE                               (APB2PERIPH_BASE + 0x0400)
#define GPIOA_BASE                              (APB2PERIPH_BASE + 0x0800)
#define GPIOC_BASE                              (APB2PERIPH_BASE + 0x1000)
#define GPIOD_BASE                              (APB2PERIPH_BASE + 0x1400)
#define TIM1_BASE                               (APB2PERIPH_BASE + 0x2C00)

// Done, now stuff taken from rv003usb follows

#define CFGLR_OFFSET 0
#define INDR_OFFSET 8
#define BSHR_OFFSET 16


#define SYSTICK_CNT 0xE000F008

// This is 6 * n + 3 cycles
.macro nx6p3delay n, freereg
    li \freereg, ((\n) + 1)
    1: c.addi \freereg, -1
    c.bnez \freereg, 1b
.endm

.macro DEBUG_TICK_SETUP
    la x4, (TIM1_BASE + 0x58) 	// for debug (Go nowhere)
.endm
.macro DEBUG_TICK_MARK
    .balign 4
    sw x0, 0(x4)
.endm
.macro RESTORE_DEBUG_MARKER x
    lw	x4, \x(sp)
.endm
.macro SAVE_DEBUG_MARKER x
	sw	x4, \x(sp)
.endm

.global rv003usb_internal_data
.global rv003usb_handle_packet
.global main
// Temp globals as we move to rust
.global done_usb_message_in
.global ret_from_se0

/* Register map
	zero, ra, sp, gp, tp, t0, t1, t2
Compressed:
	s0, s1,	a0, a1, a2, a3, a4, a5
*/

