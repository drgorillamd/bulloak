// SPDX-License-Identifier: UNLICENSED
pragma solidity 0.8.0;

import {Test} from "forge-std/Test.sol";

contract MultipleRootsTreeTest is Test {
    function test_Function1ShouldNeverRevert() external {
        vm.skip(true);
        // It should never revert.
    }

    function test_Function1WhenFirstArgIsBiggerThanSecondArg() external {
        vm.skip(true);
        // It is all good
    }

    function test_Function2WhenStuffHappens() external {
        vm.skip(true);
        // It should do something simple
    }
}