// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// Example for query_price_oracle: the ABI returns a single raw Candid nat64.
contract PriceTrade {
    address constant QUERY = address(0xffff0003);
    uint64 public lastPrice;
    uint256 public trades;

    function buy(bytes calldata compactRequest, address payable seller, uint64 maxPrice, uint256 deadline) external payable {
        require(block.timestamp <= deadline, "expired");
        (bool ok, bytes memory reply) = QUERY.call(compactRequest);
        require(ok, "oracle failed");
        require(reply.length == 15 && bytes4(reply) == 0x4449444c, "bad Candid");
        require(uint8(reply[4]) == 0 && uint8(reply[5]) == 1 && uint8(reply[6]) == 0x78, "expected nat64");
        uint64 price;
        for (uint256 i; i < 8; ++i) price |= uint64(uint8(reply[7 + i])) << (8 * i);
        require(price <= maxPrice && msg.value == price, "slippage/value");
        lastPrice = price;
        ++trades;
        (bool paid,) = seller.call{value: price}("");
        require(paid, "payment failed");
    }
}
