// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "../WrapTokenFactory.sol";
import "../WrappedAssetToken.sol";

contract ExternalMinter {
    function callMint(
        WrapTokenFactory factory,
        bytes calldata canisterId,
        address to,
        uint256 amount
    ) external returns (address) {
        return factory.mintForAsset(canisterId, to, amount);
    }
}

contract WrapTokenFactoryTest {
    function testMintDeploysAtPredictedAddressAndReusesToken() public {
        WrapTokenFactory factory = new WrapTokenFactory(address(this), 18);
        bytes memory canisterId = hex"010203040506";
        address recipient = address(0xBEEF);

        address predicted = factory.predictTokenAddress(canisterId);
        address token = factory.mintForAsset(canisterId, recipient, 7);
        require(token == predicted, "predict_mismatch");

        WrappedAssetToken wrapped = WrappedAssetToken(token);
        require(wrapped.balanceOf(recipient) == 7, "first_mint_balance");
        require(wrapped.totalSupply() == 7, "first_mint_supply");

        address tokenAgain = factory.mintForAsset(canisterId, recipient, 3);
        require(tokenAgain == token, "token_redeployed");
        require(wrapped.balanceOf(recipient) == 10, "second_mint_balance");
        require(wrapped.totalSupply() == 10, "second_mint_supply");
    }

    function testMintForAssetRequiresMinter() public {
        WrapTokenFactory factory = new WrapTokenFactory(address(this), 18);
        ExternalMinter caller = new ExternalMinter();

        (bool ok, bytes memory data) = address(caller).call(
            abi.encodeWithSelector(
                ExternalMinter.callMint.selector,
                factory,
                bytes(hex"0102"),
                address(0xCAFE),
                uint256(1)
            )
        );

        require(!ok, "expected_revert");
        require(_revertReasonEquals(data, "auth.minter_required"), "unexpected_revert_reason");
    }

    function _revertReasonEquals(bytes memory revertData, string memory expected)
        private
        pure
        returns (bool)
    {
        if (revertData.length < 68) {
            return false;
        }
        bytes4 selector;
        assembly {
            selector := mload(add(revertData, 0x20))
        }
        if (selector != 0x08c379a0) {
            return false;
        }
        bytes memory payload = new bytes(revertData.length - 4);
        for (uint256 i = 0; i < payload.length; i++) {
            payload[i] = revertData[i + 4];
        }
        string memory reason = abi.decode(payload, (string));
        return keccak256(bytes(reason)) == keccak256(bytes(expected));
    }
}
