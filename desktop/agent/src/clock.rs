// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Time for the rules: the core's trusted time (relay signed time + monotonic),
//! with the local UTC offset. Changing the Windows clock does not move this time.

use chrono::{Local, Offset};
use procastinacioff_core::api::Core;
use procastinacioff_core::clock::Clock;

/// System monotonic clock (ms since boot); the user cannot change it.
pub fn mono_ms() -> i64 {
    unsafe { windows::Win32::System::SystemInformation::GetTickCount64() as i64 }
}

pub fn device_now_ms() -> i64 {
    Local::now().timestamp_millis()
}

pub fn now_clock(core: &Core) -> Clock {
    let now = core.trusted_now_ms(device_now_ms(), mono_ms());
    Clock { now_ms: now, utc_offset_min: Local::now().offset().fix().local_minus_utc() / 60 }
}
