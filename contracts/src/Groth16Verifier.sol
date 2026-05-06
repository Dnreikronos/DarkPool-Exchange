// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

contract Groth16Verifier {
    uint256 constant SNARK_SCALAR_FIELD = 21888242871839275222246405745257275088548364400416034343698204186575808495617;
    uint256 constant PRIME_Q = 21888242871839275222246405745257275088696311157297823662689037894645226208583;

    struct VerifyingKey {
        uint256[2] alpha1;
        uint256[2][2] beta2;
        uint256[2][2] gamma2;
        uint256[2][2] delta2;
        uint256[2][] ic;
    }

    VerifyingKey internal vk;
    address public owner;
    bool public vkInitialized;

    modifier onlyOwner() {
        require(msg.sender == owner, "not owner");
        _;
    }

    constructor() {
        owner = msg.sender;
    }

    function setVerifyingKey(
        uint256[2] calldata alpha1,
        uint256[2][2] calldata beta2,
        uint256[2][2] calldata gamma2,
        uint256[2][2] calldata delta2,
        uint256[2][] calldata ic
    ) external onlyOwner {
        require(ic.length == 7, "ic length must be 7");
        vk.alpha1 = alpha1;
        vk.beta2 = beta2;
        vk.gamma2 = gamma2;
        vk.delta2 = delta2;
        delete vk.ic;
        for (uint256 i = 0; i < ic.length; i++) {
            vk.ic.push(ic[i]);
        }
        vkInitialized = true;
    }

    function verifyProof(
        uint256[2] calldata a,
        uint256[2][2] calldata b,
        uint256[2] calldata c,
        uint256[6] calldata input
    ) external view virtual returns (bool) {
        require(vkInitialized, "vk not set");

        for (uint256 i = 0; i < 6; i++) {
            require(input[i] < SNARK_SCALAR_FIELD, "input overflow");
        }

        uint256[2] memory vkX = vk.ic[0];
        for (uint256 i = 0; i < 6; i++) {
            (uint256 x, uint256 y) = ecMul(vk.ic[i + 1], input[i]);
            (vkX[0], vkX[1]) = ecAdd(vkX[0], vkX[1], x, y);
        }

        return ecPairing(
            negate(a),
            b,
            vk.alpha1,
            vk.beta2,
            vkX,
            vk.gamma2,
            c,
            vk.delta2
        );
    }

    function negate(uint256[2] calldata p) internal pure returns (uint256[2] memory) {
        if (p[0] == 0 && p[1] == 0) return [uint256(0), 0];
        return [p[0], PRIME_Q - (p[1] % PRIME_Q)];
    }

    function ecAdd(uint256 ax, uint256 ay, uint256 bx, uint256 by)
        internal view returns (uint256, uint256)
    {
        uint256[4] memory input;
        input[0] = ax;
        input[1] = ay;
        input[2] = bx;
        input[3] = by;
        bool success;
        uint256[2] memory result;
        assembly {
            success := staticcall(gas(), 0x06, input, 0x80, result, 0x40)
        }
        require(success, "ecAdd failed");
        return (result[0], result[1]);
    }

    function ecMul(uint256[2] memory p, uint256 s)
        internal view returns (uint256, uint256)
    {
        uint256[3] memory input;
        input[0] = p[0];
        input[1] = p[1];
        input[2] = s;
        bool success;
        uint256[2] memory result;
        assembly {
            success := staticcall(gas(), 0x07, input, 0x60, result, 0x40)
        }
        require(success, "ecMul failed");
        return (result[0], result[1]);
    }

    function ecPairing(
        uint256[2] memory a1, uint256[2][2] calldata a2,
        uint256[2] storage b1, uint256[2][2] storage b2,
        uint256[2] memory c1, uint256[2][2] storage c2,
        uint256[2] calldata d1, uint256[2][2] storage d2
    ) internal view returns (bool) {
        uint256[24] memory input;

        input[0]  = a1[0];
        input[1]  = a1[1];
        input[2]  = a2[0][1];
        input[3]  = a2[0][0];
        input[4]  = a2[1][1];
        input[5]  = a2[1][0];

        input[6]  = b1[0];
        input[7]  = b1[1];
        input[8]  = b2[0][1];
        input[9]  = b2[0][0];
        input[10] = b2[1][1];
        input[11] = b2[1][0];

        input[12] = c1[0];
        input[13] = c1[1];
        input[14] = c2[0][1];
        input[15] = c2[0][0];
        input[16] = c2[1][1];
        input[17] = c2[1][0];

        input[18] = d1[0];
        input[19] = d1[1];
        input[20] = d2[0][1];
        input[21] = d2[0][0];
        input[22] = d2[1][1];
        input[23] = d2[1][0];

        uint256[1] memory result;
        bool success;
        assembly {
            success := staticcall(gas(), 0x08, input, 0x300, result, 0x20)
        }
        require(success, "ecPairing failed");
        return result[0] == 1;
    }
}
