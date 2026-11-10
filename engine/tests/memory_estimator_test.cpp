#include "bigolens/estimation/memory_estimator.hpp"

#include <gtest/gtest.h>

#include <limits>

using namespace bigolens;
using estimation::StorageItem;
using estimation::StorageKind;

TEST(MemoryEstimator, StaticStorageIsSummedAndProven) {
  std::vector<StorageItem> items(2);
  items[0].kind = StorageKind::GlobalStatic;
  items[0].exactBytes = 800020;
  items[1].kind = StorageKind::GlobalStatic;
  items[1].exactBytes = 4;
  const auto m = estimation::estimateMemory(items, {});
  EXPECT_EQ(m.staticStorageBytes, 800024U);
  EXPECT_FALSE(m.staticStorageSaturated);
  EXPECT_EQ(m.estimatedMinimumStatus, models::BoundStatus::Proven);
}

TEST(MemoryEstimator, StaticStorageSaturatesInsteadOfOverflowing) {
  std::vector<StorageItem> items(2);
  for (auto& item : items) {
    item.kind = StorageKind::GlobalStatic;
    item.exactBytes = std::numeric_limits<std::uint64_t>::max() - 1;
  }
  const auto m = estimation::estimateMemory(items, {});
  EXPECT_TRUE(m.staticStorageSaturated);
  EXPECT_EQ(m.staticStorageBytes, std::numeric_limits<std::uint64_t>::max());
}

TEST(MemoryEstimator, VectorBufferEvaluatedAtConstraintMaximum) {
  std::vector<StorageItem> items(1);
  items[0].name = "v";
  items[0].kind = StorageKind::VectorBuffer;
  items[0].symbolicBytes = complexity::multiply({complexity::variable("n"), complexity::constant(8)});
  constraints::ConstraintSet cs;
  cs.variables["n"] = {1, 1000};
  const auto m = estimation::estimateMemory(items, cs);
  ASSERT_TRUE(m.estimatedMinimumBytes.has_value());
  EXPECT_DOUBLE_EQ(*m.estimatedMinimumBytes, 8000.0);
  EXPECT_EQ(m.estimatedMinimumStatus, models::BoundStatus::Estimated);
}

TEST(MemoryEstimator, UnboundedVectorLeavesMinimumUnknown) {
  std::vector<StorageItem> items(1);
  items[0].name = "v";
  items[0].kind = StorageKind::VectorBuffer;
  items[0].symbolicBytes = complexity::variable("n");
  const auto m = estimation::estimateMemory(items, {});
  EXPECT_FALSE(m.estimatedMinimumBytes.has_value());
  EXPECT_EQ(m.estimatedMinimumStatus, models::BoundStatus::Unknown);
}

TEST(MemoryEstimator, LargestLocalArrayIsTracked) {
  std::vector<StorageItem> items(2);
  items[0].kind = StorageKind::LocalArray;
  items[0].exactBytes = 100;
  items[1].kind = StorageKind::LocalArray;
  items[1].exactBytes = 4000;
  EXPECT_EQ(estimation::estimateMemory(items, {}).largestLocalArrayBytes, 4000U);
}

TEST(MemoryEstimator, SumConstraintCapsButDoesNotAccumulateBufferSize) {
  // A per-test-case vector of n ints: with n <= 1000 and sum n <= 5500 the
  // largest live instance is 1000 elements, not 5500.
  std::vector<StorageItem> items(1);
  items[0].name = "v";
  items[0].kind = StorageKind::VectorBuffer;
  items[0].symbolicBytes = complexity::multiply({complexity::variable("n"), complexity::constant(4)});
  constraints::ConstraintSet cs;
  cs.variables["t"] = {1, 100};
  cs.variables["n"] = {1, 1000};
  cs.sums.push_back({"n", "t", 5500});
  const auto m = estimation::estimateMemory(items, cs);
  ASSERT_TRUE(m.estimatedMinimumBytes.has_value());
  EXPECT_DOUBLE_EQ(*m.estimatedMinimumBytes, 4000.0);

  cs.variables["n"] = {1, 200000};
  cs.sums.front().maxTotal = 1000;  // The total also caps a single instance.
  EXPECT_DOUBLE_EQ(*estimation::estimateMemory(items, cs).estimatedMinimumBytes, 4000.0);
}
