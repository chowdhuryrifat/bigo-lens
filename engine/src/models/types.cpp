#include "bigolens/models/types.hpp"

#include "bigolens/models/finding.hpp"

namespace bigolens::models {

std::string_view toString(BoundStatus status) noexcept {
  switch (status) {
    case BoundStatus::Proven:
      return "proven";
    case BoundStatus::Estimated:
      return "estimated";
    case BoundStatus::Unknown:
      return "unknown";
    case BoundStatus::Unsupported:
      return "unsupported";
  }
  return "unknown";
}

std::string_view toString(Confidence confidence) noexcept {
  switch (confidence) {
    case Confidence::High:
      return "high";
    case Confidence::Medium:
      return "medium";
    case Confidence::Low:
      return "low";
    case Confidence::None:
      return "none";
  }
  return "none";
}

std::string_view toString(FeasibilityStatus status) noexcept {
  switch (status) {
    case FeasibilityStatus::LikelySafe:
      return "likely_safe";
    case FeasibilityStatus::Borderline:
      return "borderline";
    case FeasibilityStatus::HighRisk:
      return "high_risk";
    case FeasibilityStatus::Unknown:
      return "unknown";
  }
  return "unknown";
}

std::string_view toString(Severity severity) noexcept {
  switch (severity) {
    case Severity::Error:
      return "error";
    case Severity::Warning:
      return "warning";
    case Severity::Information:
      return "information";
  }
  return "error";
}

std::string_view toString(FindingCategory category) noexcept {
  switch (category) {
    case FindingCategory::LoopNest:
      return "loop_nest";
    case FindingCategory::Recursion:
      return "recursion";
    case FindingCategory::StaticStorage:
      return "static_storage";
    case FindingCategory::DynamicAllocation:
      return "dynamic_allocation";
    case FindingCategory::UnsupportedConstruct:
      return "unsupported_construct";
  }
  return "unsupported_construct";
}

namespace {
int rank(BoundStatus status) noexcept {
  switch (status) {
    case BoundStatus::Proven:
      return 3;
    case BoundStatus::Estimated:
      return 2;
    case BoundStatus::Unknown:
      return 1;
    case BoundStatus::Unsupported:
      return 0;
  }
  return 0;
}
}  // namespace

BoundStatus weakest(BoundStatus a, BoundStatus b) noexcept {
  return rank(a) <= rank(b) ? a : b;
}

}  // namespace bigolens::models
