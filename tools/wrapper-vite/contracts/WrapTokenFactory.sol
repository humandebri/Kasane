// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./WrappedAssetToken.sol";

/// @notice Deploys per-asset wrapped tokens by CREATE2 and mints via a single minter.
/// salt = keccak256("kasane.wrap.v1", chain_id, icrc2_canister_id_bytes)
contract WrapTokenFactory {
    string internal constant DOMAIN = "kasane.wrap.v1";
    address public immutable minter;

    mapping(bytes32 => address) public tokenByAssetKey;
    mapping(bytes32 => uint8) public decimalsByAssetKey;

    event TokenDeployed(bytes canisterId, bytes32 indexed assetKey, address indexed token, bytes32 salt);
    event Minted(bytes canisterId, bytes32 indexed assetKey, address indexed token, address to, uint256 amount);
    event Burned(bytes canisterId, bytes32 indexed assetKey, address indexed token, address from, uint256 amount);

    modifier onlyMinter() {
        require(msg.sender == minter, "auth.minter_required");
        _;
    }

    constructor(address minter_) {
        require(minter_ != address(0), "arg.minter_zero");
        minter = minter_;
    }

    function computeAssetKey(bytes calldata canisterId) public view returns (bytes32) {
        _validateCanisterId(canisterId);
        return keccak256(abi.encodePacked(DOMAIN, block.chainid, canisterId));
    }

    function computeSalt(bytes calldata canisterId) public view returns (bytes32) {
        return computeAssetKey(canisterId);
    }

    function getTokenAddress(bytes calldata canisterId) external view returns (address) {
        return tokenByAssetKey[computeAssetKey(canisterId)];
    }

    function predictTokenAddress(bytes calldata canisterId, uint8 decimals) external view returns (address predicted) {
        bytes32 salt = computeSalt(canisterId);
        bytes memory initCode = _tokenInitCode(canisterId, decimals);
        bytes32 hash = keccak256(
            abi.encodePacked(bytes1(0xff), address(this), salt, keccak256(initCode))
        );
        return address(uint160(uint256(hash)));
    }

    function mintForAsset(bytes calldata canisterId, uint8 decimals, address to, uint256 amount)
        external
        onlyMinter
        returns (address token)
    {
        require(to != address(0), "arg.to_zero");
        bytes32 assetKey = computeAssetKey(canisterId);
        token = tokenByAssetKey[assetKey];
        if (token == address(0)) {
            bytes32 salt = computeSalt(canisterId);
            token = address(new WrappedAssetToken{salt: salt}(
                _nameFor(canisterId),
                _symbolFor(canisterId),
                decimals
            ));
            tokenByAssetKey[assetKey] = token;
            decimalsByAssetKey[assetKey] = decimals;
            emit TokenDeployed(canisterId, assetKey, token, salt);
        } else {
            require(decimalsByAssetKey[assetKey] == decimals, "arg.asset_decimals_mismatch");
        }
        WrappedAssetToken(token).mint(to, amount);
        emit Minted(canisterId, assetKey, token, to, amount);
    }

    function burnFromAsset(bytes calldata canisterId, address from, uint256 amount)
        external
        onlyMinter
        returns (address token)
    {
        bytes32 assetKey = computeAssetKey(canisterId);
        token = tokenByAssetKey[assetKey];
        require(token != address(0), "unwrap.token_not_deployed");
        WrappedAssetToken(token).burnFromByFactory(from, amount);
        emit Burned(canisterId, assetKey, token, from, amount);
    }

    function _tokenInitCode(bytes calldata canisterId, uint8 decimals) private view returns (bytes memory) {
        return abi.encodePacked(
            type(WrappedAssetToken).creationCode,
            abi.encode(_nameFor(canisterId), _symbolFor(canisterId), decimals)
        );
    }

    function _nameFor(bytes calldata canisterId) private pure returns (string memory) {
        return string(abi.encodePacked("Kasane Wrapped ", _shortHex(canisterId)));
    }

    function _symbolFor(bytes calldata canisterId) private pure returns (string memory) {
        return string(abi.encodePacked("KW", _shortHex(canisterId)));
    }

    function _shortHex(bytes calldata data) private pure returns (string memory) {
        bytes32 h = keccak256(data);
        bytes memory out = new bytes(16);
        for (uint256 i = 0; i < 8; i++) {
            uint8 b = uint8(h[i]);
            out[i * 2] = _hexChar(b >> 4);
            out[i * 2 + 1] = _hexChar(b & 0x0f);
        }
        return string(out);
    }

    function _validateCanisterId(bytes calldata canisterId) private pure {
        require(canisterId.length != 0 && canisterId.length <= 29, "arg.canister_id_invalid");
    }

    function _hexChar(uint8 x) private pure returns (bytes1) {
        return x < 10 ? bytes1(x + 48) : bytes1(x + 87);
    }
}
