/// Template zk computation. Computes the sum of the secret variables.
use pbc_zk::*;

pub fn zk_compute(target: bool, position: u32) -> Sbi32 {
    let guess: Sbi32 = if position != 0 {
        Sbi32::from(1)
    } else {
        Sbi32::from(0)
    };
    for variable_id in secret_variable_ids() {
        if load_metadata::<bool>(variable_id) == target {
            let ship_placed = load_sbi::<Sbi32>(variable_id);
            return (ship_placed == guess) as Sbi32;
        }
    }
    Sbi32::from(0)
}