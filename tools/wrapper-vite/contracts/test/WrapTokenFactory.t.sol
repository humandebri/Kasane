// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "../WrapTokenFactory.sol";
import "../WrappedAssetToken.sol";

interface Vm {
    function addr(uint256 privateKey) external returns (address);
    function sign(uint256 privateKey, bytes32 digest) external returns (uint8 v, bytes32 r, bytes32 s);
    function warp(uint256 newTimestamp) external;
}

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
    Vm private constant VM = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));
    uint256 private constant PAYER_KEY = 0xa11ce;
    uint256 private constant OTHER_KEY = 0xb0b;
    uint256 private constant SECP256K1_ORDER =
        0xfffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141;

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

    function testTransferWithAuthorizationMovesBalanceAndMarksNonce() public {
        WrapTokenFactory factory = new WrapTokenFactory(address(this));
        WrappedAssetToken wrapped = WrappedAssetToken(
            factory.mintForAsset(hex"010203040506", 8, VM.addr(PAYER_KEY), 10)
        );
        address payer = VM.addr(PAYER_KEY);
        address recipient = address(0xCAFE);
        bytes32 nonce = keccak256("nonce.transfer");
        VM.warp(1_000);

        (uint8 v, bytes32 r, bytes32 s) = VM.sign(
            PAYER_KEY,
            _transferAuthorizationDigest(wrapped, payer, recipient, 4, 900, 1_100, nonce)
        );

        wrapped.transferWithAuthorization(payer, recipient, 4, 900, 1_100, nonce, v, r, s);

        require(wrapped.authorizationState(payer, nonce), "authorization_not_used");
        require(wrapped.balanceOf(payer) == 6, "payer_balance");
        require(wrapped.balanceOf(recipient) == 4, "recipient_balance");
    }

    function testReceiveWithAuthorizationRequiresRecipientCaller() public {
        WrapTokenFactory factory = new WrapTokenFactory(address(this));
        address payer = VM.addr(PAYER_KEY);
        WrappedAssetToken wrapped = WrappedAssetToken(
            factory.mintForAsset(hex"010203040506", 8, payer, 10)
        );
        bytes32 nonce = keccak256("nonce.receive");
        VM.warp(1_000);

        (uint8 v, bytes32 r, bytes32 s) = VM.sign(
            PAYER_KEY,
            _receiveAuthorizationDigest(wrapped, payer, address(0xCAFE), 4, 900, 1_100, nonce)
        );

        (bool ok, bytes memory data) = address(wrapped).call(
            abi.encodeWithSelector(
                WrappedAssetToken.receiveWithAuthorization.selector,
                payer,
                address(0xCAFE),
                uint256(4),
                uint256(900),
                uint256(1_100),
                nonce,
                v,
                r,
                s
            )
        );

        require(!ok, "expected_revert");
        require(_revertReasonEquals(data, "eip3009.recipient_required"), "unexpected_revert_reason");
    }

    function testReceiveWithAuthorizationMovesBalanceWhenCallerIsRecipient() public {
        WrapTokenFactory factory = new WrapTokenFactory(address(this));
        address payer = VM.addr(PAYER_KEY);
        WrappedAssetToken wrapped = WrappedAssetToken(
            factory.mintForAsset(hex"010203040506", 8, payer, 10)
        );
        bytes32 nonce = keccak256("nonce.receive.self");
        VM.warp(1_000);

        (uint8 v, bytes32 r, bytes32 s) = VM.sign(
            PAYER_KEY,
            _receiveAuthorizationDigest(wrapped, payer, address(this), 4, 900, 1_100, nonce)
        );

        wrapped.receiveWithAuthorization(payer, address(this), 4, 900, 1_100, nonce, v, r, s);

        require(wrapped.authorizationState(payer, nonce), "authorization_not_used");
        require(wrapped.balanceOf(payer) == 6, "payer_balance");
        require(wrapped.balanceOf(address(this)) == 4, "recipient_balance");
    }

    function testTransferWithAuthorizationRejectsReplay() public {
        WrapTokenFactory factory = new WrapTokenFactory(address(this));
        address payer = VM.addr(PAYER_KEY);
        address recipient = address(0xCAFE);
        WrappedAssetToken wrapped = WrappedAssetToken(
            factory.mintForAsset(hex"010203040506", 8, payer, 10)
        );
        bytes32 nonce = keccak256("nonce.replay");
        VM.warp(1_000);

        (uint8 v, bytes32 r, bytes32 s) = VM.sign(
            PAYER_KEY,
            _transferAuthorizationDigest(wrapped, payer, recipient, 4, 900, 1_100, nonce)
        );
        wrapped.transferWithAuthorization(payer, recipient, 4, 900, 1_100, nonce, v, r, s);

        (bool replayOk, bytes memory replayData) = address(wrapped).call(
            abi.encodeWithSelector(
                WrappedAssetToken.transferWithAuthorization.selector,
                payer,
                recipient,
                uint256(4),
                uint256(900),
                uint256(1_100),
                nonce,
                v,
                r,
                s
            )
        );
        require(!replayOk, "expected_replay_revert");
        require(_revertReasonEquals(replayData, "eip3009.authorization_used"), "unexpected_replay_revert");
    }

    function testTransferWithAuthorizationRejectsExpiredWindow() public {
        WrapTokenFactory factory = new WrapTokenFactory(address(this));
        address payer = VM.addr(PAYER_KEY);
        address recipient = address(0xCAFE);
        WrappedAssetToken wrapped = WrappedAssetToken(
            factory.mintForAsset(hex"010203040506", 8, payer, 10)
        );
        bytes32 expiredNonce = keccak256("nonce.expired");
        VM.warp(1_000);

        (uint8 expiredV, bytes32 expiredR, bytes32 expiredS) = VM.sign(
            PAYER_KEY,
            _transferAuthorizationDigest(wrapped, payer, recipient, 1, 700, 900, expiredNonce)
        );
        (bool expiredOk, bytes memory expiredData) = address(wrapped).call(
            abi.encodeWithSelector(
                WrappedAssetToken.transferWithAuthorization.selector,
                payer,
                recipient,
                uint256(1),
                uint256(700),
                uint256(900),
                expiredNonce,
                expiredV,
                expiredR,
                expiredS
            )
        );
        require(!expiredOk, "expected_expired_revert");
        require(_revertReasonEquals(expiredData, "eip3009.expired"), "unexpected_expired_revert");
    }

    function testTransferWithAuthorizationRejectsInvalidSignature() public {
        WrapTokenFactory factory = new WrapTokenFactory(address(this));
        address payer = VM.addr(PAYER_KEY);
        address recipient = address(0xCAFE);
        WrappedAssetToken wrapped = WrappedAssetToken(
            factory.mintForAsset(hex"010203040506", 8, payer, 10)
        );
        bytes32 nonce = keccak256("nonce.invalid.signature");
        VM.warp(1_000);

        (uint8 v, bytes32 r, bytes32 s) = VM.sign(
            OTHER_KEY,
            _transferAuthorizationDigest(wrapped, payer, recipient, 4, 900, 1_100, nonce)
        );
        (bool invalidOk, bytes memory invalidData) = address(wrapped).call(
            abi.encodeWithSelector(
                WrappedAssetToken.transferWithAuthorization.selector,
                payer,
                recipient,
                uint256(4),
                uint256(900),
                uint256(1_100),
                nonce,
                v,
                r,
                s
            )
        );
        require(!invalidOk, "expected_invalid_revert");
        require(_revertReasonEquals(invalidData, "eip3009.invalid_signature"), "unexpected_invalid_revert");
    }

    function testTransferWithAuthorizationRejectsHighS() public {
        WrapTokenFactory factory = new WrapTokenFactory(address(this));
        address payer = VM.addr(PAYER_KEY);
        address recipient = address(0xCAFE);
        WrappedAssetToken wrapped = WrappedAssetToken(
            factory.mintForAsset(hex"010203040506", 8, payer, 10)
        );
        bytes32 highSNonce = keccak256("nonce.high.s");
        VM.warp(1_000);

        (uint8 highV, bytes32 highR, bytes32 lowS) = VM.sign(
            PAYER_KEY,
            _transferAuthorizationDigest(wrapped, payer, recipient, 4, 900, 1_100, highSNonce)
        );
        bytes32 highS = bytes32(SECP256K1_ORDER - uint256(lowS));
        (bool highOk, bytes memory highData) = address(wrapped).call(
            abi.encodeWithSelector(
                WrappedAssetToken.transferWithAuthorization.selector,
                payer,
                recipient,
                uint256(4),
                uint256(900),
                uint256(1_100),
                highSNonce,
                highV == 27 ? uint8(28) : uint8(27),
                highR,
                highS
            )
        );
        require(!highOk, "expected_high_s_revert");
        require(_revertReasonEquals(highData, "eip3009.invalid_signature_s"), "unexpected_high_s_revert");
    }

    function testCancelAuthorizationMarksNonceWithoutTransfer() public {
        WrapTokenFactory factory = new WrapTokenFactory(address(this));
        address payer = VM.addr(PAYER_KEY);
        address recipient = address(0xCAFE);
        WrappedAssetToken wrapped = WrappedAssetToken(
            factory.mintForAsset(hex"010203040506", 8, payer, 10)
        );
        bytes32 nonce = keccak256("nonce.cancel");
        VM.warp(1_000);

        (uint8 v, bytes32 r, bytes32 s) = VM.sign(
            PAYER_KEY,
            _cancelAuthorizationDigest(wrapped, payer, nonce)
        );
        wrapped.cancelAuthorization(payer, nonce, v, r, s);

        require(wrapped.authorizationState(payer, nonce), "authorization_not_canceled");
        require(wrapped.balanceOf(payer) == 10, "payer_balance_changed");
        _assertTransferAuthorizationUsed(wrapped, payer, recipient, nonce);
    }

    function _assertTransferAuthorizationUsed(
        WrappedAssetToken wrapped,
        address payer,
        address recipient,
        bytes32 nonce
    ) private {
        (uint8 transferV, bytes32 transferR, bytes32 transferS) = VM.sign(
            PAYER_KEY,
            _transferAuthorizationDigest(wrapped, payer, recipient, 4, 900, 1_100, nonce)
        );
        (bool transferOk, bytes memory transferData) = address(wrapped).call(
            abi.encodeWithSelector(
                WrappedAssetToken.transferWithAuthorization.selector,
                payer,
                recipient,
                uint256(4),
                uint256(900),
                uint256(1_100),
                nonce,
                transferV,
                transferR,
                transferS
            )
        );
        require(!transferOk, "expected_canceled_revert");
        require(_revertReasonEquals(transferData, "eip3009.authorization_used"), "unexpected_canceled_revert");
    }

    function _transferAuthorizationDigest(
        WrappedAssetToken token,
        address from,
        address to,
        uint256 value,
        uint256 validAfter,
        uint256 validBefore,
        bytes32 nonce
    ) private view returns (bytes32) {
        return _typedDataDigest(
            token,
            keccak256(
                abi.encode(
                    token.TRANSFER_WITH_AUTHORIZATION_TYPEHASH(),
                    from,
                    to,
                    value,
                    validAfter,
                    validBefore,
                    nonce
                )
            )
        );
    }

    function _receiveAuthorizationDigest(
        WrappedAssetToken token,
        address from,
        address to,
        uint256 value,
        uint256 validAfter,
        uint256 validBefore,
        bytes32 nonce
    ) private view returns (bytes32) {
        return _typedDataDigest(
            token,
            keccak256(
                abi.encode(
                    token.RECEIVE_WITH_AUTHORIZATION_TYPEHASH(),
                    from,
                    to,
                    value,
                    validAfter,
                    validBefore,
                    nonce
                )
            )
        );
    }

    function _cancelAuthorizationDigest(
        WrappedAssetToken token,
        address authorizer,
        bytes32 nonce
    ) private view returns (bytes32) {
        return _typedDataDigest(
            token,
            keccak256(abi.encode(token.CANCEL_AUTHORIZATION_TYPEHASH(), authorizer, nonce))
        );
    }

    function _typedDataDigest(WrappedAssetToken token, bytes32 structHash)
        private
        view
        returns (bytes32)
    {
        return keccak256(abi.encodePacked("\x19\x01", token.DOMAIN_SEPARATOR(), structHash));
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
