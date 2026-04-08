// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "../WrapTokenFactory.sol";
import "../WrappedAssetToken.sol";

contract ExternalMinter {
    function callMint(
        WrapTokenFactory factory,
        bytes calldata canisterId,
        uint8 decimals,
        address to,
        uint256 amount
    ) external returns (address) {
        return factory.mintForAsset(canisterId, decimals, to, amount);
    }

    function callBurn(
        WrapTokenFactory factory,
        bytes calldata canisterId,
        address from,
        uint256 amount
    ) external returns (address) {
        return factory.burnFromAsset(canisterId, from, amount);
    }
}

contract TokenHolder {
    function approveToken(WrappedAssetToken token, address spender, uint256 amount) external {
        token.approve(spender, amount);
    }
}

contract WrapTokenFactoryTest {
    function testMintDeploysAtPredictedAddressAndReusesToken() public {
        WrapTokenFactory factory = new WrapTokenFactory(address(this));
        bytes memory canisterId = hex"010203040506";
        address recipient = address(0xBEEF);

        address predicted = factory.predictTokenAddress(canisterId, 8);
        address token = factory.mintForAsset(canisterId, 8, recipient, 7);
        require(token == predicted, "predict_mismatch");

        WrappedAssetToken wrapped = WrappedAssetToken(token);
        require(wrapped.balanceOf(recipient) == 7, "first_mint_balance");
        require(wrapped.totalSupply() == 7, "first_mint_supply");
        require(wrapped.decimals() == 8, "token_decimals");

        address tokenAgain = factory.mintForAsset(canisterId, 8, recipient, 3);
        require(tokenAgain == token, "token_redeployed");
        require(wrapped.balanceOf(recipient) == 10, "second_mint_balance");
        require(wrapped.totalSupply() == 10, "second_mint_supply");
    }

    function testMintForAssetRequiresMinter() public {
        WrapTokenFactory factory = new WrapTokenFactory(address(this));
        ExternalMinter caller = new ExternalMinter();

        (bool ok, bytes memory data) = address(caller).call(
            abi.encodeWithSelector(
                ExternalMinter.callMint.selector,
                factory,
                bytes(hex"0102"),
                uint8(8),
                address(0xCAFE),
                uint256(1)
            )
        );

        require(!ok, "expected_revert");
        require(_revertReasonEquals(data, "auth.minter_required"), "unexpected_revert_reason");
    }

    function testBurnFromAssetBurnsBalanceAndSupply() public {
        WrapTokenFactory factory = new WrapTokenFactory(address(this));
        bytes memory canisterId = hex"010203040506";
        TokenHolder holder = new TokenHolder();

        address token = factory.mintForAsset(canisterId, 8, address(holder), 10);
        WrappedAssetToken wrapped = WrappedAssetToken(token);

        holder.approveToken(wrapped, address(factory), 6);

        address burnedToken = factory.burnFromAsset(canisterId, address(holder), 6);
        require(burnedToken == token, "burn_token_mismatch");
        require(wrapped.balanceOf(address(holder)) == 4, "burn_balance");
        require(wrapped.totalSupply() == 4, "burn_supply");
    }

    function testBurnFromAssetRequiresAllowance() public {
        WrapTokenFactory factory = new WrapTokenFactory(address(this));
        bytes memory canisterId = hex"010203040506";
        TokenHolder holder = new TokenHolder();

        factory.mintForAsset(canisterId, 8, address(holder), 10);

        (bool ok, bytes memory data) = address(factory).call(
            abi.encodeWithSelector(
                WrapTokenFactory.burnFromAsset.selector,
                canisterId,
                address(holder),
                uint256(6)
            )
        );

        require(!ok, "expected_revert");
        require(_revertReasonEquals(data, "erc20.insufficient_allowance"), "unexpected_revert_reason");
    }

    function testBurnFromAssetRejectsMissingToken() public {
        WrapTokenFactory factory = new WrapTokenFactory(address(this));
        (bool ok, bytes memory data) = address(factory).call(
            abi.encodeWithSelector(
                WrapTokenFactory.burnFromAsset.selector,
                bytes(hex"9999"),
                address(0xBEEF),
                uint256(1)
            )
        );

        require(!ok, "expected_revert");
        require(_revertReasonEquals(data, "unwrap.token_not_deployed"), "unexpected_revert_reason");
    }

    function testDirectBurnIsDisabled() public {
        WrapTokenFactory factory = new WrapTokenFactory(address(this));
        bytes memory canisterId = hex"010203040506";
        address token = factory.mintForAsset(canisterId, 8, address(this), 10);

        (bool ok, bytes memory data) = token.call(
            abi.encodeWithSelector(WrappedAssetToken.burn.selector, uint256(1))
        );

        require(!ok, "expected_revert");
        require(_revertReasonEquals(data, "disabled.use_factory"), "unexpected_revert_reason");
    }

    function testDirectBurnFromIsDisabled() public {
        WrapTokenFactory factory = new WrapTokenFactory(address(this));
        bytes memory canisterId = hex"010203040506";
        TokenHolder holder = new TokenHolder();
        address token = factory.mintForAsset(canisterId, 8, address(holder), 10);
        WrappedAssetToken wrapped = WrappedAssetToken(token);
        holder.approveToken(wrapped, address(this), 5);

        (bool ok, bytes memory data) = token.call(
            abi.encodeWithSelector(WrappedAssetToken.burnFrom.selector, address(holder), uint256(1))
        );

        require(!ok, "expected_revert");
        require(_revertReasonEquals(data, "disabled.use_factory"), "unexpected_revert_reason");
    }

    function testMintRejectsInvalidCanisterId() public {
        WrapTokenFactory factory = new WrapTokenFactory(address(this));
        bytes memory emptyCanisterId = bytes("");
        bytes memory tooLongCanisterId = new bytes(30);

        (bool emptyOk, bytes memory emptyData) = address(factory).call(
            abi.encodeWithSelector(
                WrapTokenFactory.mintForAsset.selector,
                emptyCanisterId,
                uint8(8),
                address(0xBEEF),
                uint256(1)
            )
        );
        require(!emptyOk, "expected_empty_revert");
        require(_revertReasonEquals(emptyData, "arg.canister_id_invalid"), "unexpected_empty_revert");

        (bool longOk, bytes memory longData) = address(factory).call(
            abi.encodeWithSelector(
                WrapTokenFactory.mintForAsset.selector,
                tooLongCanisterId,
                uint8(8),
                address(0xBEEF),
                uint256(1)
            )
        );
        require(!longOk, "expected_long_revert");
        require(_revertReasonEquals(longData, "arg.canister_id_invalid"), "unexpected_long_revert");
    }

    function testMintRejectsDecimalsMismatch() public {
        WrapTokenFactory factory = new WrapTokenFactory(address(this));
        bytes memory canisterId = hex"010203040506";
        factory.mintForAsset(canisterId, 8, address(0xBEEF), 1);

        (bool ok, bytes memory data) = address(factory).call(
            abi.encodeWithSelector(
                WrapTokenFactory.mintForAsset.selector,
                canisterId,
                uint8(18),
                address(0xCAFE),
                uint256(1)
            )
        );

        require(!ok, "expected_revert");
        require(_revertReasonEquals(data, "arg.asset_decimals_mismatch"), "unexpected_revert_reason");
    }

    function testNameAndSymbolUseLongerSuffix() public {
        WrapTokenFactory factory = new WrapTokenFactory(address(this));
        bytes memory canisterId = hex"010203040506";
        address token = factory.mintForAsset(canisterId, 8, address(this), 1);
        WrappedAssetToken wrapped = WrappedAssetToken(token);

        require(bytes(wrapped.symbol()).length == 18, "symbol_suffix_length");
        require(bytes(wrapped.name()).length == 31, "name_suffix_length");
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
