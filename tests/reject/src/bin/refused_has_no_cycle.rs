//! C5: cycles are compile errors, so there is no runtime refusal for them.
use weft::Refused;

fn main() {
    #[cfg(not(twin))]
    let _ = Refused::Cycle; // ~ERROR E0599
    #[cfg(twin)]
    let _ = Refused::Held;
}
