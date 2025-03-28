#[cfg(target_arch = "x86_64")]
mod x64;

#[cfg(not(target_arch = "x86_64"))]
mod stubs;

#[cfg(target_arch = "x86_64")]
pub use x64::*;

#[cfg(not(target_arch = "x86_64"))]
pub use stubs::*;
