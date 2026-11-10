#pragma once

#include <cstdint>
#include <optional>
#include <string>
#include <string_view>

namespace bigolens::models {

/// How strongly a reported bound is supported. Independent from Confidence.
enum class BoundStatus {
  Proven,      ///< Follows from the analyzed code under the stated assumptions.
  Estimated,   ///< Derived with heuristics or unverified assumptions.
  Unknown,     ///< The analysis could not determine a bound.
  Unsupported  ///< The construct is outside what the analyzer supports.
};

/// Subjective reliability of a result. Never implies proof.
enum class Confidence { High, Medium, Low, None };

enum class FeasibilityStatus { LikelySafe, Borderline, HighRisk, Unknown };

enum class Severity { Error, Warning, Information };

[[nodiscard]] std::string_view toString(BoundStatus status) noexcept;
[[nodiscard]] std::string_view toString(Confidence confidence) noexcept;
[[nodiscard]] std::string_view toString(FeasibilityStatus status) noexcept;
[[nodiscard]] std::string_view toString(Severity severity) noexcept;

/// Returns the weaker of two bound statuses (Proven > Estimated > Unknown > Unsupported).
[[nodiscard]] BoundStatus weakest(BoundStatus a, BoundStatus b) noexcept;

/// 1-based source position. `file` is the display path supplied by the client.
struct SourceLocation {
  std::string file;
  std::uint32_t line = 0;
  std::uint32_t column = 0;
  std::optional<std::uint32_t> endLine;
  std::optional<std::uint32_t> endColumn;
};

struct Diagnostic {
  Severity severity = Severity::Error;
  std::string message;
  std::optional<SourceLocation> location;
};

}  // namespace bigolens::models
