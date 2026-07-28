// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {PoseidonBN254} from "../src/PoseidonBN254.sol";

/// Cross-language verification that the Solidity Poseidon port reproduces the
/// arkworks circuit hash byte-for-byte (#209). The reference values are emitted
/// by `cargo run -p dp-zk-cli -- export-poseidon` and locked on the Rust side
/// in `dp_zk_cli::export_poseidon::tests::reference_vectors_are_stable`, which
/// in turn flows through the shared `dp_zk::settlement_chain` that the circuit's
/// `settlement_acc_matches_native_chain` test pins to the in-circuit gadget.
/// If any of those layers drifts, this test fails before a desync can reach a
/// deployment.
contract PoseidonBN254Test is Test {
    uint256 internal constant V_POSEIDON5 =
        17202221768493126381180179227250714636677047994833944093631184439391269770327;
    uint256 internal constant V_CHAIN2 =
        13176863867183045156691872925194891460694388274017740522862028043601348896898;

    /// Raw 5-input sponge hash: poseidon([1, 2, 3, 4, 5]).
    function test_poseidon5_matches_arkworks() public pure {
        assertEq(PoseidonBN254.poseidon5(1, 2, 3, 4, 5), V_POSEIDON5, "poseidon5(1..5) mismatch");
    }

    /// Two-row settlement chain (acc0 = 0) over the reference rows in
    /// `export_poseidon::reference_rows` (addresses and 1e8-scaled amounts).
    function test_settlement_chain_two_rows_matches_arkworks() public pure {
        uint256 acc = 0;
        acc = PoseidonBN254.poseidon5(acc, 0x10, 0x20, 10_000_000_000, 1_000_000_000);
        acc = PoseidonBN254.poseidon5(acc, 0x11, 0x21, 20_000_000_000, 2_000_000_000);
        assertEq(acc, V_CHAIN2, "2-row settlement chain mismatch");
    }

    /// Order sensitivity: swapping the two rows must change the accumulator, so
    /// an operator cannot reorder settled matches without breaking the binding.
    function test_chain_is_order_sensitive() public pure {
        uint256 forward = 0;
        forward = PoseidonBN254.poseidon5(forward, 0x10, 0x20, 10_000_000_000, 1_000_000_000);
        forward = PoseidonBN254.poseidon5(forward, 0x11, 0x21, 20_000_000_000, 2_000_000_000);

        uint256 reversed = 0;
        reversed = PoseidonBN254.poseidon5(reversed, 0x11, 0x21, 20_000_000_000, 2_000_000_000);
        reversed = PoseidonBN254.poseidon5(reversed, 0x10, 0x20, 10_000_000_000, 1_000_000_000);

        assertTrue(forward != reversed, "chain must be order-sensitive");
    }
}
