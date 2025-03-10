//! Demonstrates very basic RTIC support for imxrt-ral
//!
//! Flash this program to your Teensy 4, and observe a blinking LED.
//!
//! # Limitations
//!
//! - There's `unsafe` code to set VTOR. It relies on implementation details of
//!   the runtime. You should rely on your runtime for this behavior...
//! - Code executes out of flash (XIP), so interrupt latency is bad. Again, you
//!   should rely on your runtime to place code in TCM...

#![no_main]
#![no_std]

use teensy4_fcb as _;
use teensy4_panic as _;

#[rtic::app(device = rtic_safe, peripherals = true)]
mod app {
    use imxrt_ral as ral;

    const LED_OFFSET: u32 = 3;
    const LED: u32 = 1 << LED_OFFSET;

    /// Microseconds, given the clock selection and configuration
    /// for the timer.
    const PIT_PERIOD_US: u32 = 1_000_000;

    #[local]
    struct Local {
        gpio2: ral::gpio::GPIO2,
        pit: ral::pit::PIT,
    }

    #[shared]
    struct Shared {}

    #[init]
    fn init(
        init::Context {
            device: rtic_safe::Peripherals(device),
            ..
        }: init::Context,
    ) -> (Shared, Local, init::Monotonics) {
        let iomuxc = device.IOMUXC;
        // Set the GPIO pad to a GPIO function (ALT 5)
        ral::write_reg!(ral::iomuxc, iomuxc, SW_MUX_CTL_PAD_GPIO_B0_03, 5);
        // Increase drive strength, but leave other fields at their current value...
        ral::modify_reg!(
            ral::iomuxc,
            iomuxc,
            SW_PAD_CTL_PAD_GPIO_B0_03,
            DSE: DSE_7_R0_7
        );

        let gpio2 = device.GPIO2;
        // Set GPIO2[3] to an output
        ral::modify_reg!(ral::gpio, gpio2, GDIR, |gdir| gdir | LED);

        let ccm = device.CCM;

        // Disable the PIT clock gate while we change the clock...
        ral::modify_reg!(ral::ccm, ccm, CCGR1, CG6: 0b00);
        // Set the periodic clock divider, selection.
        // 24MHz crystal oscillator, divided by 24 == 1MHz PIT clock
        ral::modify_reg!(
            ral::ccm,
            ccm,
            CSCMR1,
            PERCLK_PODF: DIVIDE_24,
            PERCLK_CLK_SEL: PERCLK_CLK_SEL_1 // Oscillator clock
        );
        // Re-enable PIT clock
        ral::modify_reg!(ral::ccm, ccm, CCGR1, CG6: 0b11);

        let pit = device.PIT;
        // Disable the PIT, just in case it was used by the boot ROM
        ral::write_reg!(ral::pit, pit, MCR, MDIS: MDIS_1);
        // Reset channel 0 control; we'll use channel 0 for our timer
        ral::write_reg!(ral::pit::timer, &pit.TIMER[0], TCTRL, 0);
        // Set the counter value
        ral::write_reg!(ral::pit::timer, &pit.TIMER[0], LDVAL, PIT_PERIOD_US);
        // Enable the PIT timer
        ral::modify_reg!(ral::pit, pit, MCR, MDIS: MDIS_0);
        // Enable interrupts and start counting
        ral::write_reg!(ral::pit::timer, &pit.TIMER[0], TCTRL, TIE: 1);
        ral::modify_reg!(ral::pit::timer, &pit.TIMER[0], TCTRL, TEN: 1);

        ral::write_reg!(ral::gpio, gpio2, DR_SET, LED);
        (Shared {}, Local { gpio2, pit }, init::Monotonics())
    }

    #[task(binds = PIT, local = [gpio2, pit])]
    fn pit(cx: pit::Context) {
        let pit = cx.local.pit;
        let gpio2 = cx.local.gpio2;

        if ral::read_reg!(ral::pit::timer, &pit.TIMER[0], TFLG, TIF == 1) {
            ral::write_reg!(ral::pit::timer, &pit.TIMER[0], TFLG, TIF: 1);
            ral::write_reg!(ral::gpio, gpio2, DR_TOGGLE, LED);
        }

        cortex_m::asm::dsb();
    }
}

#[cortex_m_rt::pre_init]
unsafe fn pre_init() {
    extern "C" {
        static __reset_vector: u32;
    }
    const SCB_VTOR: *mut u32 = 0xE000_ED08 as *mut u32;
    // Offset by 4 to point at start of stack
    core::ptr::write_volatile(SCB_VTOR, (&__reset_vector as *const _ as u32) - 4);
}
