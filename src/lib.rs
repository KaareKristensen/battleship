//! Battleship contract.


#![allow(unused_variables)]

#[macro_use]
extern crate pbc_contract_codegen;
extern crate pbc_contract_common;
extern crate pbc_lib;

use pbc_contract_common::address::Address;
use pbc_contract_common::context::ContractContext;
use pbc_contract_common::events::EventGroup;
use pbc_contract_common::zk::{CalculationStatus, SecretVarId, ZkClosed, ZkInputDef, ZkState, ZkStateChange};
use read_write_rpc_derive::ReadWriteRPC;
use read_write_state_derive::ReadWriteState;

/// Secret variable metadata. Unused for this contract, so we use a zero-sized struct to save space.
#[derive(ReadWriteState, ReadWriteRPC, Debug)]
struct SecretVarMetadata {
    player: bool,
}

/// The maximum size of MPC variables.
const BITLENGTH_OF_SECRET_VARIABLES: u32 = 32;

#[derive(ReadWriteRPC, Debug)]
#[state]
struct ContractState {
    player_a: Address,
    player_b: Address,
    next_turn: Address,
    winner: Option<Address>,
    hit_a: Option<bool>,
    hit_b: Option<bool>,
    game_state: String,
}

/// INIT
#[init (zk = true)]
fn initialize(ctx: ContractContext, zk_state: ZkState<SecretVarMetadata>, player_a: Address, player_b: Address) -> ContractState {
    ContractState {
        player_a,
        player_b,
        next_turn: player_a,
        winner: None,
        hit_a: None,
        hit_b: None,
        game_state: "Setup".to_string(),
    }
}


#[zk_on_secret_input(shortname = 0x40)]
fn setup_board(
    context: ContractContext,
    state: ContractState,
    zk_state: ZkState<SecretVarMetadata>,
) -> (
    ContractState,
    Vec<EventGroup>,
    ZkInputDef<SecretVarMetadata>,
) {
    let player_id = get_player_id(context.sender, &state);

    let input_def = ZkInputDef {
        seal: false,
        metadata: SecretVarMetadata { player: player_id },
        expected_bit_lengths: vec![BITLENGTH_OF_SECRET_VARIABLES],
    };
    (state, vec![], input_def)
}

fn get_player_id(sender: Address, state: &ContractState) -> bool {
    if sender == state.player_a {
        false
    } else if sender == state.player_b {
        true
    } else {
        panic!("{:?} is not a player", sender);
    }
}

fn get_player_address(id: bool, state: &ContractState) -> Address {
    if id {
        state.player_b
    } else {
        state.player_a
    }
}

#[action(shortname = 0x01, zk = true)]
fn shoot(
    context: ContractContext,
    state: ContractState,
    zk_state: ZkState<SecretVarMetadata>,
    position: u32,
) -> (ContractState, Vec<EventGroup>, Vec<ZkStateChange>) {
    assert_eq!(
        context.sender, state.next_turn,
        "Its not your turn"
    );

    assert_eq!(state.game_state, "Playing".to_string(), "Game is not ready to be played");

    assert_eq!(
        zk_state.calculation_state,
        CalculationStatus::Waiting,
        "Computation must start from Waiting state, but was {:?}",
        zk_state.calculation_state,
    );

    let player_id = get_player_id(context.sender, &state);
    let output_variable_metadata: Vec<SecretVarMetadata> = vec![
        SecretVarMetadata {
            player: !player_id,
        }
    ];
    (
        state,
        vec![],
        vec![ZkStateChange::start_computation_with_inputs(output_variable_metadata, vec![
            !player_id,
            position,
        ])],
    )
}

#[zk_on_compute_complete]
fn auction_compute_complete(
    ontext: ContractContext,
    state: ContractState,
    zk_state: ZkState<SecretVarMetadata>,
    output_variables: Vec<SecretVarId>,
) -> (ContractState, Vec<EventGroup>, Vec<ZkStateChange>) {
    (
        state,
        vec![],
        vec![ZkStateChange::OpenVariables {
            variables: output_variables,
        }],
    )
}

#[zk_on_variables_opened]
fn open_auction_variable(
    context: ContractContext,
    mut state: ContractState,
    zk_state: ZkState<SecretVarMetadata>,
    opened_variables: Vec<SecretVarId>,
) -> (ContractState, Vec<EventGroup>, Vec<ZkStateChange>) {
    assert_eq!(
        opened_variables.len(),
        1,
        "Unexpected number of output variables"
    );

    let was_ship = read_variable_u32_le(&zk_state, opened_variables.get(0));
    let x: &ZkClosed<SecretVarMetadata> = zk_state.get_variable(opened_variables.get(0).unwrap().clone()).unwrap();
    let shot_at = get_player_address(x.metadata.player, &state);

    if shot_at == state.player_a {
        state.hit_a = Some(was_ship != 0);
    } else {
        state.hit_b = Some(was_ship != 0);
    }

    if state.hit_a.is_some() && state.hit_b.is_some() {
        state.game_state = "ENDED".to_string();
        state.winner = calculate_winner(&state);
    }

    state.next_turn = shot_at;

    (state, vec![], vec![ZkStateChange::OutputComplete { variables_to_delete: vec![] }])
}

fn calculate_winner(state: &ContractState) -> Option<Address> {
    let hit_a = state.hit_a.unwrap();
    let hit_b = state.hit_b.unwrap();
    if hit_a && hit_b {
        None
    } else if hit_a {
        Some(state.player_b)
    } else if hit_b {
        Some(state.player_a)
    } else {
        None
    }
}

#[zk_on_variable_inputted]
fn inputted_variable(
    context: ContractContext,
    mut state: ContractState,
    zk_state: ZkState<SecretVarMetadata>,
    inputted_variable: SecretVarId,
) -> ContractState {
    let amount_of_boards = zk_state.secret_variables.len() as u32;
    if amount_of_boards == 2 {
        state.game_state = "Playing".to_string();
    }
    state
}


/// Reads a variable's data as an u32.
///
/// ### Parameters:
///
/// * `zk_state`: [`&ZkState<SecretVarMetadata>`], the current zk state.
///
/// * `sum_variable_id`: [`Option<&SecretVarId>`], the id of the secret variable to be read.
///
/// ### Returns
/// The value of the variable as an [`u32`].
fn read_variable_u32_le(
    zk_state: &ZkState<SecretVarMetadata>,
    sum_variable_id: Option<&SecretVarId>,
) -> u32 {
    let sum_variable_id = *sum_variable_id.unwrap();
    let sum_variable = zk_state.get_variable(sum_variable_id).unwrap();
    let mut buffer = [0u8; 4];
    buffer.copy_from_slice(sum_variable.data.as_ref().unwrap().as_slice());
    <u32>::from_le_bytes(buffer)
}
