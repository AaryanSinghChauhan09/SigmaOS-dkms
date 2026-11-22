//! SigmaOS component binary: dkms

use sigmaos_dkms::Component;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Component::new().operate()?;
    println!("sigmaos-dkms: ok");
    Ok(())
}
