#include "bigolens/feasibility/feasibility.hpp"

#include <gtest/gtest.h>

using namespace bigolens;
using feasibility::RuntimeEstimate;
using models::FeasibilityStatus;

TEST(Feasibility, TimeUnknownWithoutEstimateOrLimit) {
  EXPECT_EQ(feasibility::assessTime(std::nullopt, 1000).status, FeasibilityStatus::Unknown);
  EXPECT_EQ(feasibility::assessTime(RuntimeEstimate{1, 2, 3}, std::nullopt).status, FeasibilityStatus::Unknown);
}

TEST(Feasibility, TimeClassification) {
  EXPECT_EQ(feasibility::assessTime(RuntimeEstimate{100, 200, 400}, 1000).status, FeasibilityStatus::LikelySafe);
  EXPECT_EQ(feasibility::assessTime(RuntimeEstimate{400, 700, 1200}, 1000).status, FeasibilityStatus::Borderline);
  EXPECT_EQ(feasibility::assessTime(RuntimeEstimate{1500, 2000, 3000}, 1000).status, FeasibilityStatus::HighRisk);
}

TEST(Feasibility, MemoryHighRiskFromStaticStorageAlone) {
  estimation::MemoryEstimate m;
  m.staticStorageBytes = 300LL * 1024 * 1024;
  EXPECT_EQ(feasibility::assessMemory(m, std::nullopt, 256LL * 1024 * 1024).status, FeasibilityStatus::HighRisk);
}

TEST(Feasibility, MemoryNeverLikelySafeFromLowerBoundsOnly) {
  estimation::MemoryEstimate m;
  m.staticStorageBytes = 1024;
  m.estimatedMinimumBytes = 1024;
  EXPECT_EQ(feasibility::assessMemory(m, std::nullopt, 256LL * 1024 * 1024).status, FeasibilityStatus::Unknown);
}

TEST(Feasibility, MemoryWithKnownUpperBound) {
  estimation::MemoryEstimate m;
  const std::int64_t limit = 100;
  EXPECT_EQ(feasibility::assessMemory(m, 50.0, limit).status, FeasibilityStatus::LikelySafe);
  EXPECT_EQ(feasibility::assessMemory(m, 90.0, limit).status, FeasibilityStatus::Borderline);
  EXPECT_EQ(feasibility::assessMemory(m, 150.0, limit).status, FeasibilityStatus::HighRisk);
}
