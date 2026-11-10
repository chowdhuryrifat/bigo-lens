// Clang LibTooling frontend. This is the only translation unit that includes
// Clang headers; nothing Clang-specific crosses the SourceAnalyzer interface.
//
// Scope (see docs/specifications/COMPLEXITY_MODEL.md):
//  - canonical counted `for` loops (+=c, -=c, *=c, /=c, <<=, >>=, i*i<=n),
//  - `while (t--)` test-case loops and simple count-down / halving `while` loops,
//  - range-based `for` over a named container or a constant-size array,
//  - loop bounds that depend on enclosing loop indices (triangular nests),
//  - direct recursion detection (reported as Unsupported),
//  - global/static storage, local constant-size arrays, sized std::vector construction.
// Everything else is reported as Unknown or Unsupported, never guessed.
#include "bigolens/parser/clang_frontend.hpp"

#include <clang/AST/ASTConsumer.h>
#include <clang/AST/ASTContext.h>
#include <clang/AST/Decl.h>
#include <clang/AST/DeclCXX.h>
#include <clang/AST/DeclTemplate.h>
#include <clang/AST/Expr.h>
#include <clang/AST/ExprCXX.h>
#include <clang/AST/RecursiveASTVisitor.h>
#include <clang/AST/Stmt.h>
#include <clang/AST/StmtCXX.h>
#include <clang/Basic/Diagnostic.h>
#include <clang/Basic/FileManager.h>
#include <clang/Basic/SourceManager.h>
#include <clang/Basic/Version.h>
#include <clang/Frontend/CompilerInstance.h>
#include <clang/Frontend/FrontendAction.h>
#include <clang/Tooling/Tooling.h>
#include <llvm/ADT/SmallString.h>
#include <llvm/Support/MemoryBuffer.h>
#include <llvm/Support/VirtualFileSystem.h>

#include <algorithm>
#include <cstdint>
#include <cstdlib>
#include <filesystem>
#include <map>
#include <optional>
#include <set>
#include <string>
#include <tuple>
#include <utility>
#include <vector>

#include "bigolens/complexity/expression.hpp"

#ifndef BIGOLENS_DEFAULT_CLANG_RESOURCE_DIR
#define BIGOLENS_DEFAULT_CLANG_RESOURCE_DIR ""
#endif

namespace bigolens::parser {
namespace {

namespace cx = complexity;
using models::BoundStatus;
using models::Confidence;

constexpr const char* kVirtualFileName = "bigolens-input.cpp";
constexpr const char* kLoopProvenance = "clang-loop-analyzer";
constexpr const char* kRecursionProvenance = "clang-recursion-detector";
constexpr const char* kStorageProvenance = "clang-storage-analyzer";
constexpr std::size_t kMaxDiagnostics = 50;

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

class CollectingDiagnostics final : public clang::DiagnosticConsumer {
 public:
  explicit CollectingDiagnostics(std::string displayPath) : displayPath_(std::move(displayPath)) {}

  void HandleDiagnostic(clang::DiagnosticsEngine::Level level, const clang::Diagnostic& info) override {
    clang::DiagnosticConsumer::HandleDiagnostic(level, info);
    if (level != clang::DiagnosticsEngine::Error && level != clang::DiagnosticsEngine::Fatal) {
      return;
    }
    if (diagnostics_.size() >= kMaxDiagnostics) {
      truncated_ = true;
      return;
    }
    llvm::SmallString<256> text;
    info.FormatDiagnostic(text);
    models::Diagnostic diagnostic;
    diagnostic.severity = models::Severity::Error;
    diagnostic.message = std::string(text.str());
    if (info.hasSourceManager() && info.getLocation().isValid()) {
      const clang::PresumedLoc presumed = info.getSourceManager().getPresumedLoc(info.getLocation());
      if (presumed.isValid()) {
        models::SourceLocation location;
        const std::string file = presumed.getFilename();
        location.file = file == kVirtualFileName ? displayPath_ : file;
        location.line = presumed.getLine();
        location.column = presumed.getColumn();
        diagnostic.location = std::move(location);
      }
    }
    diagnostics_.push_back(std::move(diagnostic));
  }

  [[nodiscard]] std::vector<models::Diagnostic> take() {
    if (truncated_) {
      diagnostics_.push_back({models::Severity::Information,
                              "further compiler errors omitted (limit " + std::to_string(kMaxDiagnostics) + ")",
                              std::nullopt});
    }
    return std::move(diagnostics_);
  }

 private:
  std::string displayPath_;
  std::vector<models::Diagnostic> diagnostics_;
  bool truncated_ = false;
};

// ---------------------------------------------------------------------------
// Helpers over the Clang AST
// ---------------------------------------------------------------------------

const clang::VarDecl* referencedVar(const clang::Expr* expr) {
  if (expr == nullptr) {
    return nullptr;
  }
  if (const auto* ref = llvm::dyn_cast<clang::DeclRefExpr>(expr->IgnoreParenImpCasts())) {
    return llvm::dyn_cast<clang::VarDecl>(ref->getDecl());
  }
  return nullptr;
}

std::optional<std::int64_t> constantValue(const clang::Expr* expr, const clang::ASTContext& ctx) {
  if (expr == nullptr || expr->isValueDependent() || expr->isTypeDependent()) {
    return std::nullopt;
  }
  clang::Expr::EvalResult result;
  if (!expr->EvaluateAsInt(result, ctx)) {
    return std::nullopt;
  }
  const llvm::APSInt& value = result.Val.getInt();
  if (value.getSignificantBits() > 63) {
    return std::nullopt;
  }
  return value.getExtValue();
}

bool isLoop(const clang::Stmt* stmt) {
  return llvm::isa<clang::ForStmt, clang::WhileStmt, clang::DoStmt, clang::CXXForRangeStmt>(stmt);
}

/// Loops reachable from `stmt` without passing through another loop.
void collectChildLoops(const clang::Stmt* stmt, std::vector<const clang::Stmt*>& out) {
  if (stmt == nullptr) {
    return;
  }
  for (const clang::Stmt* child : stmt->children()) {
    if (child == nullptr) {
      continue;
    }
    if (isLoop(child)) {
      out.push_back(child);
    } else {
      collectChildLoops(child, out);
    }
  }
}

/// True if `stmt` (excluding nested loops/switches for `break`) can leave the loop early.
bool hasEarlyExit(const clang::Stmt* stmt, bool insideNestedBreakTarget) {
  if (stmt == nullptr) {
    return false;
  }
  if (llvm::isa<clang::ReturnStmt, clang::GotoStmt, clang::CXXThrowExpr>(stmt)) {
    return true;
  }
  if (llvm::isa<clang::BreakStmt>(stmt)) {
    return !insideNestedBreakTarget;
  }
  const bool nested = insideNestedBreakTarget || isLoop(stmt) || llvm::isa<clang::SwitchStmt>(stmt);
  for (const clang::Stmt* child : stmt->children()) {
    if (hasEarlyExit(child, nested)) {
      return true;
    }
  }
  return false;
}

bool isNonConstReference(clang::QualType type) {
  if (const auto* ref = type->getAs<clang::ReferenceType>()) {
    return !ref->getPointeeType().isConstQualified();
  }
  return false;
}

/// Conservative check: is `var` (possibly) written inside `stmt`?
bool mayModify(const clang::Stmt* stmt, const clang::VarDecl* var) {
  if (stmt == nullptr || var == nullptr) {
    return false;
  }
  if (const auto* unary = llvm::dyn_cast<clang::UnaryOperator>(stmt)) {
    if ((unary->isIncrementDecrementOp() || unary->getOpcode() == clang::UO_AddrOf) &&
        referencedVar(unary->getSubExpr()) == var) {
      return true;
    }
  } else if (const auto* binary = llvm::dyn_cast<clang::BinaryOperator>(stmt)) {
    if (binary->isAssignmentOp() && referencedVar(binary->getLHS()) == var) {
      return true;
    }
  } else if (const auto* call = llvm::dyn_cast<clang::CallExpr>(stmt)) {
    const clang::FunctionDecl* callee = call->getDirectCallee();
    const auto* method = callee ? llvm::dyn_cast<clang::CXXMethodDecl>(callee) : nullptr;
    const bool memberOperator = llvm::isa<clang::CXXOperatorCallExpr>(call) && method != nullptr;
    for (unsigned i = 0; i < call->getNumArgs(); ++i) {
      if (referencedVar(call->getArg(i)) != var) {
        continue;
      }
      if (memberOperator && i == 0) {
        if (!method->isConst()) {
          return true;  // e.g. `x += 1` on a class type.
        }
        continue;
      }
      const unsigned paramIndex = memberOperator ? i - 1 : i;
      if (callee == nullptr || paramIndex >= callee->getNumParams()) {
        return true;  // Dependent or variadic call: cannot rule out modification.
      }
      if (isNonConstReference(callee->getParamDecl(paramIndex)->getType())) {
        return true;
      }
    }
  }
  for (const clang::Stmt* child : stmt->children()) {
    if (mayModify(child, var)) {
      return true;
    }
  }
  return false;
}

std::string qualifiedRecordName(clang::QualType type) {
  type = type.getNonReferenceType().getCanonicalType();
  if (const auto* record = type->getAsCXXRecordDecl()) {
    return record->getQualifiedNameAsString();
  }
  return {};
}

/// Maps libstdc++/libc++ inline namespaces (std::__1::vector, std::__cxx11::...) to std::name.
std::string normaliseStdName(std::string name) {
  for (const char* inlineNs : {"std::__1::", "std::__cxx11::", "std::__debug::"}) {
    const std::string prefix = inlineNs;
    if (name.rfind(prefix, 0) == 0) {
      name = "std::" + name.substr(prefix.size());
    }
  }
  return name;
}

// ---------------------------------------------------------------------------
// Loop analysis
// ---------------------------------------------------------------------------

struct LoopIndex {
  const clang::VarDecl* decl;
  std::string name;
};

struct NestResult {
  cx::ExprPtr cost;  ///< Null when the count is unknown.
  BoundStatus status = BoundStatus::Proven;
  int depth = 0;
  bool dependentBound = false;
  bool logarithmic = false;
  bool squareRoot = false;
  bool testCaseLoop = false;
  bool rangeFor = false;
  bool earlyExit = false;
  std::vector<std::string> assumptions;
  std::vector<std::string> limitations;
  std::string unknownReason;

  void fail(BoundStatus newStatus, std::string reason) {
    status = models::weakest(status, newStatus);
    cost = nullptr;
    if (unknownReason.empty()) {
      unknownReason = std::move(reason);
    }
  }
};

void appendUnique(std::vector<std::string>& into, const std::vector<std::string>& from) {
  for (const auto& item : from) {
    if (std::find(into.begin(), into.end(), item) == into.end()) {
      into.push_back(item);
    }
  }
}

class LoopAnalyzer {
 public:
  explicit LoopAnalyzer(clang::ASTContext& ctx) : ctx_(ctx) {}

  NestResult analyzeNest(const clang::Stmt* loop) {
    NestResult result = analyzeLoop(loop);
    collectCallLimitations(loop, result);
    return result;
  }

 private:
  /// Converts an integer expression to a symbolic expression, or null.
  cx::ExprPtr toSymbolic(const clang::Expr* expr, std::set<const clang::VarDecl*>* referenced = nullptr) {
    if (expr == nullptr) {
      return nullptr;
    }
    if (auto value = constantValue(expr, ctx_)) {
      return cx::constant(static_cast<double>(*value));
    }
    const clang::Expr* e = expr->IgnoreParenCasts();
    if (const clang::VarDecl* var = referencedVar(e)) {
      if (dataVars_.count(var) != 0) {
        return nullptr;  // Value read from data (range-for element): not a size.
      }
      if (referenced != nullptr) {
        referenced->insert(var);
      }
      for (const auto& index : indices_) {
        if (index.decl == var) {
          return cx::variable(index.name);
        }
      }
      return cx::variable(var->getNameAsString());
    }
    if (const auto* call = llvm::dyn_cast<clang::CXXMemberCallExpr>(e)) {
      const auto* method = call->getMethodDecl();
      const std::string name = method ? method->getNameAsString() : std::string();
      if ((name == "size" || name == "length") && call->getNumArgs() == 0) {
        if (const clang::VarDecl* object = referencedVar(call->getImplicitObjectArgument())) {
          if (referenced != nullptr) {
            referenced->insert(object);
          }
          return cx::variable(object->getNameAsString() + ".size()");
        }
      }
      return nullptr;
    }
    if (const auto* binary = llvm::dyn_cast<clang::BinaryOperator>(e)) {
      cx::ExprPtr lhs = toSymbolic(binary->getLHS(), referenced);
      cx::ExprPtr rhs = toSymbolic(binary->getRHS(), referenced);
      if (!lhs || !rhs) {
        return nullptr;
      }
      switch (binary->getOpcode()) {
        case clang::BO_Add:
          return cx::add({lhs, rhs});
        case clang::BO_Sub:
          return cx::subtract(lhs, rhs);
        case clang::BO_Mul:
          return cx::multiply({lhs, rhs});
        case clang::BO_Div:
          // Real division bounds integer (floor) division from above for non-negative operands.
          if (rhs->kind() == cx::ExprKind::Constant && rhs->number() > 0.0) {
            return cx::multiply({lhs, cx::constant(1.0 / rhs->number())});
          }
          return nullptr;
        default:
          return nullptr;
      }
    }
    return nullptr;
  }

  bool mentionsIndex(const cx::ExprPtr& expr) const {
    if (!expr) {
      return false;
    }
    const auto free = cx::freeVariables(*expr);
    return std::any_of(indices_.begin(), indices_.end(),
                       [&](const LoopIndex& index) { return free.count(index.name) != 0; });
  }

  /// Cost of a loop body: the sum of its child loops, or 1 for a straight-line body.
  NestResult analyzeBody(const clang::Stmt* body) {
    NestResult result;
    std::vector<const clang::Stmt*> children;
    if (body != nullptr && isLoop(body)) {
      children.push_back(body);  // Unbraced body that is itself a loop.
    } else {
      collectChildLoops(body, children);
    }
    if (children.empty()) {
      result.cost = cx::constant(1.0);
      return result;
    }
    std::vector<cx::ExprPtr> terms;
    for (const clang::Stmt* child : children) {
      NestResult childResult = analyzeLoop(child);
      result.status = models::weakest(result.status, childResult.status);
      result.depth = std::max(result.depth, childResult.depth);
      result.dependentBound = result.dependentBound || childResult.dependentBound;
      result.logarithmic = result.logarithmic || childResult.logarithmic;
      result.squareRoot = result.squareRoot || childResult.squareRoot;
      result.rangeFor = result.rangeFor || childResult.rangeFor;
      result.earlyExit = result.earlyExit || childResult.earlyExit;
      appendUnique(result.assumptions, childResult.assumptions);
      appendUnique(result.limitations, childResult.limitations);
      if (result.unknownReason.empty()) {
        result.unknownReason = childResult.unknownReason;
      }
      if (childResult.cost) {
        terms.push_back(childResult.cost);
      } else {
        terms.clear();
        result.cost = nullptr;
        break;
      }
    }
    if (!terms.empty()) {
      result.cost = cx::add(std::move(terms));
    }
    if (children.size() > 1 && result.cost) {
      result.assumptions.push_back("sequential inner loops are summed");
    }
    return result;
  }

  NestResult analyzeLoop(const clang::Stmt* loop) {
    if (const auto* forStmt = llvm::dyn_cast<clang::ForStmt>(loop)) {
      return analyzeFor(*forStmt);
    }
    if (const auto* whileStmt = llvm::dyn_cast<clang::WhileStmt>(loop)) {
      return analyzeWhile(*whileStmt);
    }
    if (const auto* rangeFor = llvm::dyn_cast<clang::CXXForRangeStmt>(loop)) {
      return analyzeRangeFor(*rangeFor);
    }
    NestResult result;
    result.depth = 1;
    result.fail(BoundStatus::Unsupported, "do-while loops are not modelled yet");
    return result;
  }

  /// Combines a loop's iteration space with its body cost.
  /// When `index` is set and the body depends on it, a Summation over
  /// [lower, upper] is produced; otherwise trips * body.
  NestResult combine(const clang::Stmt* body, const std::optional<LoopIndex>& index, const cx::ExprPtr& lower,
                     const cx::ExprPtr& upper, const cx::ExprPtr& trips, bool indexSummable) {
    if (index) {
      indices_.push_back(*index);
    }
    NestResult result = analyzeBody(body);
    if (index) {
      indices_.pop_back();
    }
    result.depth += 1;
    if (hasEarlyExit(body, false)) {
      result.earlyExit = true;
      appendUnique(result.assumptions,
                   {"an early exit (break/return/goto/throw) may end a loop sooner; the count is an upper bound"});
    }
    if (!result.cost) {
      return result;
    }
    const bool bodyUsesIndex = index && cx::freeVariables(*result.cost).count(index->name) != 0;
    if (bodyUsesIndex) {
      if (!indexSummable) {
        result.fail(BoundStatus::Unknown,
                    "inner loop bounds depend on an index whose iteration space is not a unit-step range");
        return result;
      }
      result.dependentBound = true;
      result.cost = cx::summation(index->name, lower, upper, result.cost);
    } else {
      result.cost = cx::multiply({trips, result.cost});
    }
    return result;
  }

  NestResult analyzeFor(const clang::ForStmt& loop) {
    NestResult unknown;
    unknown.depth = 1;

    // --- init: `T i = start` or `i = start`.
    const clang::VarDecl* indexVar = nullptr;
    const clang::Expr* startExpr = nullptr;
    if (const auto* declStmt = llvm::dyn_cast_or_null<clang::DeclStmt>(loop.getInit())) {
      if (declStmt->isSingleDecl()) {
        if (const auto* var = llvm::dyn_cast<clang::VarDecl>(declStmt->getSingleDecl())) {
          indexVar = var;
          startExpr = var->getInit();
        }
      }
    } else if (const auto* assign = llvm::dyn_cast_or_null<clang::BinaryOperator>(loop.getInit())) {
      if (assign->getOpcode() == clang::BO_Assign) {
        indexVar = referencedVar(assign->getLHS());
        startExpr = assign->getRHS();
      }
    }
    if (indexVar == nullptr || startExpr == nullptr || loop.getCond() == nullptr || loop.getInc() == nullptr) {
      unknown.fail(BoundStatus::Unknown, "loop is not in canonical counted form (init; condition; increment)");
      return unknown;
    }
    if (!indexVar->getType()->isIntegerType()) {
      unknown.fail(BoundStatus::Unsupported, "only integer loop indices are modelled");
      return unknown;
    }

    // --- increment.
    enum class Step { Add, Sub, Mul, Div, Unknown };
    Step step = Step::Unknown;
    std::int64_t stepValue = 0;
    const clang::Expr* inc = loop.getInc()->IgnoreParenImpCasts();
    if (const auto* unary = llvm::dyn_cast<clang::UnaryOperator>(inc)) {
      if (referencedVar(unary->getSubExpr()) == indexVar) {
        step = unary->isIncrementOp() ? Step::Add : (unary->isDecrementOp() ? Step::Sub : Step::Unknown);
        stepValue = 1;
      }
    } else if (const auto* compound = llvm::dyn_cast<clang::CompoundAssignOperator>(inc)) {
      if (referencedVar(compound->getLHS()) == indexVar) {
        if (auto value = constantValue(compound->getRHS(), ctx_)) {
          stepValue = *value;
          switch (compound->getOpcode()) {
            case clang::BO_AddAssign:
              step = Step::Add;
              break;
            case clang::BO_SubAssign:
              step = Step::Sub;
              break;
            case clang::BO_MulAssign:
              step = Step::Mul;
              break;
            case clang::BO_DivAssign:
              step = Step::Div;
              break;
            case clang::BO_ShlAssign:
            case clang::BO_ShrAssign:
              if (stepValue >= 1 && stepValue <= 30) {
                step = compound->getOpcode() == clang::BO_ShlAssign ? Step::Mul : Step::Div;
                stepValue = std::int64_t{1} << stepValue;
              }
              break;
            default:
              break;
          }
        }
      }
    }
    if (step == Step::Unknown || stepValue <= 0) {
      unknown.fail(BoundStatus::Unknown, "loop increment is not a constant step on the index");
      return unknown;
    }
    if ((step == Step::Mul || step == Step::Div) && stepValue < 2) {
      unknown.fail(BoundStatus::Unknown, "multiplicative step below 2 does not terminate geometrically");
      return unknown;
    }

    // --- condition: `i OP bound`, `bound OP i` or `i * i OP bound`.
    const auto* cond = llvm::dyn_cast<clang::BinaryOperator>(loop.getCond()->IgnoreParenImpCasts());
    if (cond == nullptr || !cond->isComparisonOp()) {
      unknown.fail(BoundStatus::Unknown, "loop condition is not a comparison against the index");
      return unknown;
    }
    clang::BinaryOperatorKind op = cond->getOpcode();
    const clang::Expr* boundExpr = nullptr;
    bool squareRoot = false;
    if (referencedVar(cond->getLHS()) == indexVar) {
      boundExpr = cond->getRHS();
    } else if (referencedVar(cond->getRHS()) == indexVar) {
      boundExpr = cond->getLHS();
      op = clang::BinaryOperator::reverseComparisonOp(op);
    } else if (const auto* mul = llvm::dyn_cast<clang::BinaryOperator>(cond->getLHS()->IgnoreParenImpCasts())) {
      if (mul->getOpcode() == clang::BO_Mul && referencedVar(mul->getLHS()) == indexVar &&
          referencedVar(mul->getRHS()) == indexVar) {
        squareRoot = true;
        boundExpr = cond->getRHS();
      }
    }
    if (boundExpr == nullptr) {
      unknown.fail(BoundStatus::Unknown, "loop condition does not compare the index with a bound");
      return unknown;
    }

    std::set<const clang::VarDecl*> boundVars;
    cx::ExprPtr start = toSymbolic(startExpr, &boundVars);
    cx::ExprPtr bound = toSymbolic(boundExpr, &boundVars);
    if (!start || !bound) {
      unknown.fail(BoundStatus::Unknown, "loop start or bound is not a supported integer expression");
      return unknown;
    }

    // --- the body must not write the index; written bound variables weaken the result.
    if (mayModify(loop.getBody(), indexVar)) {
      unknown.fail(BoundStatus::Unsupported,
                   "loop index '" + indexVar->getNameAsString() + "' is modified in the body");
      return unknown;
    }
    std::vector<std::string> extraLimitations;
    BoundStatus extraStatus = BoundStatus::Proven;
    for (const clang::VarDecl* var : boundVars) {
      if (var != indexVar && mayModify(loop.getBody(), var)) {
        extraStatus = BoundStatus::Estimated;
        extraLimitations.push_back("bound variable '" + var->getNameAsString() +
                                   "' may be modified inside the loop; the count assumes its initial value");
      }
    }

    const std::optional<LoopIndex> index = LoopIndex{indexVar, indexVar->getNameAsString()};
    const auto one = cx::constant(1.0);
    std::vector<std::string> assumptions;
    NestResult result;

    if (squareRoot) {
      if (step != Step::Add || stepValue != 1 || (op != clang::BO_LT && op != clang::BO_LE)) {
        unknown.fail(BoundStatus::Unknown, "square-root loop shape not supported");
        return unknown;
      }
      const auto trips = cx::add({cx::power(bound, 0.5), one});
      assumptions.push_back("index starts at a non-negative value; at most sqrt(bound) + 1 iterations");
      result = combine(loop.getBody(), index, nullptr, nullptr, trips, false);
      result.squareRoot = true;
    } else if (step == Step::Add) {
      cx::ExprPtr upper;  // Last index value (inclusive).
      if (op == clang::BO_LT || (op == clang::BO_NE && stepValue == 1)) {
        upper = cx::subtract(bound, one);
      } else if (op == clang::BO_LE) {
        upper = bound;
      } else {
        unknown.fail(BoundStatus::Unknown, "increasing index with a condition that does not bound it from above");
        return unknown;
      }
      cx::ExprPtr trips = cx::add({cx::subtract(upper, start), one});
      if (stepValue != 1) {
        trips = cx::add(
            {cx::multiply({cx::subtract(upper, start), cx::constant(1.0 / static_cast<double>(stepValue))}), one});
      }
      assumptions.push_back("loop bound assumed to be at least the start value");
      result = combine(loop.getBody(), index, start, upper, trips, stepValue == 1);
    } else if (step == Step::Sub) {
      cx::ExprPtr lower;  // Last index value (inclusive).
      if (op == clang::BO_GT || (op == clang::BO_NE && stepValue == 1)) {
        lower = cx::add({bound, one});
      } else if (op == clang::BO_GE) {
        lower = bound;
      } else {
        unknown.fail(BoundStatus::Unknown, "decreasing index with a condition that does not bound it from below");
        return unknown;
      }
      if (op == clang::BO_GE && indexVar->getType()->isUnsignedIntegerType() && cx::isConstant(*bound, 0.0)) {
        unknown.fail(BoundStatus::Unsupported, "unsigned index compared with >= 0 never terminates");
        return unknown;
      }
      cx::ExprPtr trips = cx::add({cx::subtract(start, lower), one});
      if (stepValue != 1) {
        trips = cx::add(
            {cx::multiply({cx::subtract(start, lower), cx::constant(1.0 / static_cast<double>(stepValue))}), one});
      }
      assumptions.push_back("loop start assumed to be at least the bound");
      result = combine(loop.getBody(), index, lower, start, trips, stepValue == 1);
    } else if (step == Step::Mul) {
      const auto startValue = constantValue(startExpr, ctx_);
      if (!startValue || *startValue < 1 || (op != clang::BO_LT && op != clang::BO_LE)) {
        unknown.fail(BoundStatus::Unknown, "geometric loop needs a constant start >= 1 and an upper bound");
        return unknown;
      }
      const auto trips = cx::add({cx::logarithm(bound, static_cast<double>(stepValue)), one});
      assumptions.push_back("geometric index growth: at most log_" + std::to_string(stepValue) +
                            "(bound) + 1 iterations");
      result = combine(loop.getBody(), index, nullptr, nullptr, trips, false);
      result.logarithmic = true;
    } else {  // Step::Div
      const auto boundValue = constantValue(boundExpr, ctx_);
      const bool towardsZero =
          boundValue && ((op == clang::BO_GT && *boundValue == 0) || (op == clang::BO_GE && *boundValue == 1) ||
                         (op == clang::BO_NE && *boundValue == 0));
      if (!towardsZero) {
        unknown.fail(BoundStatus::Unknown, "halving loop must run while the index is positive");
        return unknown;
      }
      const auto trips = cx::add({cx::logarithm(start, static_cast<double>(stepValue)), one});
      assumptions.push_back("geometric index decay: at most log_" + std::to_string(stepValue) +
                            "(start) + 1 iterations");
      result = combine(loop.getBody(), index, nullptr, nullptr, trips, false);
      result.logarithmic = true;
    }

    result.status = models::weakest(result.status, extraStatus);
    appendUnique(result.assumptions, assumptions);
    appendUnique(result.limitations, extraLimitations);
    return result;
  }

  NestResult analyzeWhile(const clang::WhileStmt& loop) {
    NestResult unknown;
    unknown.depth = 1;
    const clang::Expr* cond = loop.getCond() ? loop.getCond()->IgnoreParenImpCasts() : nullptr;

    // `while (t--)` and `while (t-- > 0)`.
    const clang::UnaryOperator* postDec = llvm::dyn_cast_or_null<clang::UnaryOperator>(cond);
    if (const auto* cmp = llvm::dyn_cast_or_null<clang::BinaryOperator>(cond)) {
      const auto rhs = constantValue(cmp->getRHS(), ctx_);
      if (cmp->getOpcode() == clang::BO_GT && rhs && *rhs == 0) {
        postDec = llvm::dyn_cast<clang::UnaryOperator>(cmp->getLHS()->IgnoreParenImpCasts());
      }
    }
    if (postDec != nullptr && postDec->getOpcode() == clang::UO_PostDec) {
      if (const clang::VarDecl* counter = referencedVar(postDec->getSubExpr())) {
        if (mayModify(loop.getBody(), counter)) {
          unknown.fail(BoundStatus::Unsupported,
                       "counter '" + counter->getNameAsString() + "' is modified in the body");
          return unknown;
        }
        NestResult result =
            combine(loop.getBody(), std::nullopt, nullptr, nullptr, cx::variable(counter->getNameAsString()), false);
        result.testCaseLoop = true;
        appendUnique(result.assumptions,
                     {"'" + counter->getNameAsString() + "' counts iterations (assumed non-negative at loop entry)"});
        return result;
      }
    }

    // `while (x > 0)`, `while (x)`, `while (x != 0)`, `while (x >= 1)` with one update of x.
    const clang::VarDecl* var = referencedVar(cond);
    if (const auto* cmp = llvm::dyn_cast_or_null<clang::BinaryOperator>(cond); cmp != nullptr && var == nullptr) {
      const auto rhs = constantValue(cmp->getRHS(), ctx_);
      if (rhs && ((cmp->getOpcode() == clang::BO_GT && *rhs == 0) || (cmp->getOpcode() == clang::BO_NE && *rhs == 0) ||
                  (cmp->getOpcode() == clang::BO_GE && *rhs == 1))) {
        var = referencedVar(cmp->getLHS());
      }
    }
    if (var != nullptr && var->getType()->isIntegerType()) {
      enum class Update { None, Decrement, Divide, Other };
      Update update = Update::None;
      std::int64_t divisor = 0;
      int updates = 0;
      // Top-level statements of the body; an unbraced body is a single statement.
      std::vector<const clang::Stmt*> statements;
      if (const auto* compound = llvm::dyn_cast_or_null<clang::CompoundStmt>(loop.getBody())) {
        statements.assign(compound->body_begin(), compound->body_end());
      } else if (loop.getBody() != nullptr) {
        statements.push_back(loop.getBody());
      }
      {
        for (const clang::Stmt* stmt : statements) {
          const auto* expr = llvm::dyn_cast<clang::Expr>(stmt);
          if (expr == nullptr) {
            if (mayModify(stmt, var)) {
              update = Update::Other;
            }
            continue;
          }
          expr = expr->IgnoreParenImpCasts();
          if (const auto* unary = llvm::dyn_cast<clang::UnaryOperator>(expr);
              unary != nullptr && unary->isDecrementOp() && referencedVar(unary->getSubExpr()) == var) {
            update = updates == 0 ? Update::Decrement : Update::Other;
            ++updates;
            continue;
          }
          if (const auto* compound = llvm::dyn_cast<clang::CompoundAssignOperator>(expr);
              compound != nullptr && referencedVar(compound->getLHS()) == var) {
            const auto value = constantValue(compound->getRHS(), ctx_);
            ++updates;
            if (value && compound->getOpcode() == clang::BO_SubAssign && *value == 1) {
              update = updates == 1 ? Update::Decrement : Update::Other;
            } else if (value && compound->getOpcode() == clang::BO_DivAssign && *value >= 2) {
              update = updates == 1 ? Update::Divide : Update::Other;
              divisor = *value;
            } else if (value && compound->getOpcode() == clang::BO_ShrAssign && *value >= 1 && *value <= 30) {
              update = updates == 1 ? Update::Divide : Update::Other;
              divisor = std::int64_t{1} << *value;
            } else {
              update = Update::Other;
            }
            continue;
          }
          if (mayModify(expr, var)) {
            update = Update::Other;
          }
        }
      }
      const std::string name = var->getNameAsString();
      if (update == Update::Decrement || update == Update::Divide) {
        const auto trips =
            update == Update::Decrement
                ? cx::variable(name)
                : cx::add({cx::logarithm(cx::variable(name), static_cast<double>(divisor)), cx::constant(1.0)});
        NestResult result = combine(loop.getBody(), std::nullopt, nullptr, nullptr, trips, false);
        result.logarithmic = result.logarithmic || update == Update::Divide;
        appendUnique(result.assumptions, {"'" + name + "' holds its value at loop entry and is non-negative"});
        return result;
      }
    }

    unknown.fail(BoundStatus::Unknown,
                 "data-dependent termination: the iteration count depends on values computed in the loop");
    return unknown;
  }

  NestResult analyzeRangeFor(const clang::CXXForRangeStmt& loop) {
    NestResult unknown;
    unknown.depth = 1;
    const clang::Expr* range = loop.getRangeInit();
    cx::ExprPtr trips;
    if (range != nullptr) {
      const clang::QualType type = range->IgnoreParenImpCasts()->getType();
      if (const auto* array = ctx_.getAsConstantArrayType(type)) {
        trips = cx::constant(static_cast<double>(array->getSize().getZExtValue()));
      } else if (const clang::VarDecl* container = referencedVar(range)) {
        trips = cx::variable(container->getNameAsString() + ".size()");
      }
    }
    if (!trips) {
      unknown.fail(BoundStatus::Unknown, "range-based for over an expression that is not a named container");
      return unknown;
    }
    if (const clang::VarDecl* element = loop.getLoopVariable()) {
      dataVars_.insert(element);
    }
    NestResult result = combine(loop.getBody(), std::nullopt, nullptr, nullptr, trips, false);
    result.rangeFor = true;
    appendUnique(result.assumptions, {"the container is not resized during iteration"});
    return result;
  }

  void collectCallLimitations(const clang::Stmt* stmt, NestResult& result) {
    std::set<std::string> userCalls;
    std::set<std::string> libraryCalls;
    std::vector<const clang::Stmt*> stack{stmt};
    while (!stack.empty()) {
      const clang::Stmt* current = stack.back();
      stack.pop_back();
      if (current == nullptr) {
        continue;
      }
      if (const auto* rangeFor = llvm::dyn_cast<clang::CXXForRangeStmt>(current)) {
        // The implicit begin()/end()/iterator calls of the loop header are part
        // of the iteration itself; only the range expression (evaluated once)
        // and the body contain calls written by the user.
        stack.push_back(rangeFor->getRangeInit());
        stack.push_back(rangeFor->getBody());
        continue;
      }
      if (const auto* call = llvm::dyn_cast<clang::CallExpr>(current)) {
        if (const clang::FunctionDecl* callee = call->getDirectCallee()) {
          const clang::FunctionDecl* definition = callee->getDefinition();
          const bool userDefined = definition != nullptr &&
                                   ctx_.getSourceManager().isInMainFile(definition->getLocation()) &&
                                   !callee->isImplicit();
          if (userDefined) {
            userCalls.insert(callee->getNameAsString());
          } else if (!llvm::isa<clang::CXXOperatorCallExpr>(call) && !llvm::isa<clang::CXXConstructorDecl>(callee)) {
            libraryCalls.insert(describeLibraryCall(*call, *callee));
          }
        }
      }
      for (const clang::Stmt* child : current->children()) {
        stack.push_back(child);
      }
    }
    for (const auto& name : userCalls) {
      result.status = models::weakest(result.status, BoundStatus::Estimated);
      result.limitations.push_back("calls user function '" + name +
                                   "'; its cost is not included (interprocedural analysis is not implemented)");
    }
    if (!libraryCalls.empty()) {
      std::string list;
      std::size_t shown = 0;
      for (const auto& name : libraryCalls) {
        if (shown++ == 6) {
          list += ", ...";
          break;
        }
        list += (list.empty() ? "" : ", ") + name;
      }
      result.limitations.push_back("per-iteration cost of library calls is not included: " + list);
    }
  }

  static std::string describeLibraryCall(const clang::CallExpr& call, const clang::FunctionDecl& callee) {
    std::string name = normaliseStdName(callee.getQualifiedNameAsString());
    if (const auto* member = llvm::dyn_cast<clang::CXXMemberCallExpr>(&call)) {
      const std::string record = normaliseStdName(qualifiedRecordName(member->getObjectType()));
      if (!record.empty()) {
        name = record + "::" + callee.getNameAsString();
      }
    }
    return name;
  }

  clang::ASTContext& ctx_;
  std::vector<LoopIndex> indices_;
  std::set<const clang::VarDecl*> dataVars_;
};

// ---------------------------------------------------------------------------
// Translation-unit walker
// ---------------------------------------------------------------------------

class AnalysisVisitor final : public clang::RecursiveASTVisitor<AnalysisVisitor> {
 public:
  AnalysisVisitor(clang::ASTContext& ctx, std::string displayPath, analyzers::SourceAnalysis& out)
      : ctx_(ctx), displayPath_(std::move(displayPath)), out_(out) {}

  bool VisitFunctionDecl(clang::FunctionDecl* function) {
    if (function == nullptr || !function->doesThisDeclarationHaveABody() || function->isInvalidDecl() ||
        !inMainFile(function->getLocation()) || function->isImplicit()) {
      return true;
    }
    analyzeFunction(*function);
    return true;
  }

  bool VisitVarDecl(clang::VarDecl* var) {
    if (var == nullptr || var->isInvalidDecl() || !inMainFile(var->getLocation()) ||
        llvm::isa<clang::ParmVarDecl>(var)) {
      return true;
    }
    const clang::QualType type = var->getType();
    if (type->isDependentType() || type->isIncompleteType() || type->isUndeducedType() || type->isReferenceType()) {
      return true;
    }
    const bool global = var->hasGlobalStorage() && var->isThisDeclarationADefinition() == clang::VarDecl::Definition;
    if (global && type->isConstantSizeType()) {
      estimation::StorageItem item;
      item.name = var->getNameAsString();
      item.kind = estimation::StorageKind::GlobalStatic;
      item.typeName = type.getAsString(ctx_.getPrintingPolicy());
      item.exactBytes = static_cast<std::uint64_t>(ctx_.getTypeSizeInChars(type).getQuantity());
      item.location = location(var->getBeginLoc(), var->getEndLoc());
      if (ctx_.getAsConstantArrayType(type) != nullptr) {
        addStorageFinding(item, models::FindingCategory::StaticStorage, "static-array",
                          cx::constant(static_cast<double>(*item.exactBytes)));
      }
      out_.storage.push_back(std::move(item));
    } else if (!global && var->hasLocalStorage() && ctx_.getAsConstantArrayType(type) != nullptr &&
               type->isConstantSizeType()) {
      estimation::StorageItem item;
      item.name = var->getNameAsString();
      item.kind = estimation::StorageKind::LocalArray;
      item.typeName = type.getAsString(ctx_.getPrintingPolicy());
      item.exactBytes = static_cast<std::uint64_t>(ctx_.getTypeSizeInChars(type).getQuantity());
      item.location = location(var->getBeginLoc(), var->getEndLoc());
      out_.storage.push_back(std::move(item));
    }
    analyzeVectorConstruction(*var);
    return true;
  }

 private:
  bool inMainFile(clang::SourceLocation loc) const {
    return loc.isValid() && ctx_.getSourceManager().isInMainFile(ctx_.getSourceManager().getExpansionLoc(loc));
  }

  models::SourceLocation location(clang::SourceLocation begin, clang::SourceLocation end) const {
    const auto& sm = ctx_.getSourceManager();
    models::SourceLocation result;
    result.file = displayPath_;
    const clang::PresumedLoc b = sm.getPresumedLoc(sm.getExpansionLoc(begin));
    if (b.isValid()) {
      result.line = b.getLine();
      result.column = b.getColumn();
    }
    const clang::PresumedLoc e = sm.getPresumedLoc(sm.getExpansionLoc(end));
    if (e.isValid()) {
      result.endLine = e.getLine();
      result.endColumn = e.getColumn();
    }
    return result;
  }

  std::string nextId() { return "F" + std::to_string(++findingCounter_); }

  void addStorageFinding(const estimation::StorageItem& item, models::FindingCategory category, std::string pattern,
                         cx::ExprPtr bytes, BoundStatus status = BoundStatus::Proven) {
    models::Finding finding;
    finding.id = nextId();
    finding.category = category;
    finding.pattern = std::move(pattern);
    finding.functionName = currentFunction_;
    finding.location = item.location;
    finding.spaceExpression = std::move(bytes);
    finding.boundStatus = status;
    finding.confidence = status == BoundStatus::Proven ? Confidence::High : Confidence::Medium;
    finding.provenance = kStorageProvenance;
    if (category == models::FindingCategory::StaticStorage) {
      finding.description = "static-storage array '" + item.name + "' of type " + item.typeName +
                            " (size from the compiler's type layout)";
    } else {
      finding.description = "std::vector '" + item.name + "' constructed with a symbolic element count";
      finding.assumptions.push_back("element buffer only; std::vector bookkeeping and allocator overhead excluded");
      finding.limitations.push_back("elements that own further heap memory (e.g. nested containers) are not followed");
    }
    out_.findings.push_back(std::move(finding));
  }

  void analyzeVectorConstruction(const clang::VarDecl& var) {
    const clang::QualType type = var.getType();
    if (normaliseStdName(qualifiedRecordName(type)) != "std::vector") {
      return;
    }
    const auto* construct =
        llvm::dyn_cast_or_null<clang::CXXConstructExpr>(var.getInit() ? var.getInit()->IgnoreImplicit() : nullptr);
    if (construct == nullptr || construct->getNumArgs() == 0 || !construct->getArg(0)->getType()->isIntegerType()) {
      return;
    }
    const auto* specialization =
        llvm::dyn_cast_or_null<clang::ClassTemplateSpecializationDecl>(type.getCanonicalType()->getAsCXXRecordDecl());
    if (specialization == nullptr || specialization->getTemplateArgs().size() == 0) {
      return;
    }
    const clang::QualType element = specialization->getTemplateArgs()[0].getAsType();
    if (element.isNull() || element->isDependentType() || element->isIncompleteType()) {
      return;
    }
    const auto elementBytes = static_cast<double>(ctx_.getTypeSizeInChars(element).getQuantity());

    LoopAnalyzerSizeConverter converter(ctx_);
    cx::ExprPtr count = converter.convert(construct->getArg(0));
    if (!count) {
      return;
    }
    estimation::StorageItem item;
    item.name = var.getNameAsString();
    item.kind = estimation::StorageKind::VectorBuffer;
    item.typeName = type.getAsString(ctx_.getPrintingPolicy());
    item.symbolicBytes = cx::multiply({count, cx::constant(elementBytes)});
    item.location = location(var.getBeginLoc(), var.getEndLoc());
    addStorageFinding(item, models::FindingCategory::DynamicAllocation, "sized-vector", item.symbolicBytes,
                      BoundStatus::Estimated);
    out_.storage.push_back(std::move(item));
  }

  /// Minimal size-expression converter for allocation sizes (constants, variables, + - *).
  class LoopAnalyzerSizeConverter {
   public:
    explicit LoopAnalyzerSizeConverter(const clang::ASTContext& ctx) : ctx_(ctx) {}
    cx::ExprPtr convert(const clang::Expr* expr) const {
      if (auto value = constantValue(expr, ctx_)) {
        return *value >= 0 ? cx::constant(static_cast<double>(*value)) : nullptr;
      }
      const clang::Expr* e = expr->IgnoreParenCasts();
      if (const clang::VarDecl* var = referencedVar(e)) {
        return cx::variable(var->getNameAsString());
      }
      if (const auto* binary = llvm::dyn_cast<clang::BinaryOperator>(e)) {
        auto lhs = convert(binary->getLHS());
        auto rhs = convert(binary->getRHS());
        if (!lhs || !rhs) {
          return nullptr;
        }
        if (binary->getOpcode() == clang::BO_Add) {
          return cx::add({lhs, rhs});
        }
        if (binary->getOpcode() == clang::BO_Mul) {
          return cx::multiply({lhs, rhs});
        }
      }
      return nullptr;
    }

   private:
    const clang::ASTContext& ctx_;
  };

  void analyzeFunction(const clang::FunctionDecl& function) {
    currentFunction_ = function.getNameAsString();
    detectRecursion(function);

    std::vector<const clang::Stmt*> loops;
    collectChildLoops(function.getBody(), loops);
    for (const clang::Stmt* loop : loops) {
      LoopAnalyzer analyzer(ctx_);
      NestResult nest = analyzer.analyzeNest(loop);

      models::Finding finding;
      finding.id = nextId();
      finding.category = models::FindingCategory::LoopNest;
      finding.functionName = currentFunction_;
      finding.location = location(loop->getBeginLoc(), loop->getEndLoc());
      finding.provenance = kLoopProvenance;
      finding.assumptions = nest.assumptions;
      finding.limitations = nest.limitations;
      finding.timeExpression = nest.cost;
      finding.boundStatus = nest.cost ? nest.status : models::weakest(nest.status, BoundStatus::Unknown);

      if (nest.testCaseLoop) {
        finding.pattern = "test-case-loop";
      } else if (!nest.cost) {
        finding.pattern = nest.status == BoundStatus::Unsupported ? "unsupported-loop" : "data-dependent-loop";
      } else if (nest.dependentBound) {
        finding.pattern = "triangular-loop";
      } else if (nest.depth > 1) {
        finding.pattern = "nested-loop";
      } else if (nest.logarithmic) {
        finding.pattern = "logarithmic-loop";
      } else if (nest.squareRoot) {
        finding.pattern = "sqrt-loop";
      } else if (nest.rangeFor) {
        finding.pattern = "range-for-loop";
      } else {
        finding.pattern = "linear-loop";
      }

      if (nest.cost) {
        finding.description = "loop nest of depth " + std::to_string(nest.depth) +
                              "; time expression counts executions of the innermost loop bodies";
      } else {
        finding.description = "loop nest whose iteration count could not be determined: " + nest.unknownReason;
        finding.limitations.insert(finding.limitations.begin(), nest.unknownReason);
      }

      switch (finding.boundStatus) {
        case BoundStatus::Proven:
          finding.confidence = finding.limitations.empty() && !nest.earlyExit ? Confidence::High : Confidence::Medium;
          break;
        case BoundStatus::Estimated:
          finding.confidence = Confidence::Low;
          break;
        case BoundStatus::Unknown:
        case BoundStatus::Unsupported:
          finding.confidence = Confidence::None;
          break;
      }
      out_.findings.push_back(std::move(finding));
    }
  }

  void detectRecursion(const clang::FunctionDecl& function) {
    const clang::FunctionDecl* canonical = function.getCanonicalDecl();
    std::size_t selfCalls = 0;
    std::vector<const clang::Stmt*> stack{function.getBody()};
    while (!stack.empty()) {
      const clang::Stmt* current = stack.back();
      stack.pop_back();
      if (current == nullptr) {
        continue;
      }
      if (const auto* call = llvm::dyn_cast<clang::CallExpr>(current)) {
        if (const clang::FunctionDecl* callee = call->getDirectCallee();
            callee != nullptr && callee->getCanonicalDecl() == canonical) {
          ++selfCalls;
        }
      }
      for (const clang::Stmt* child : current->children()) {
        stack.push_back(child);
      }
    }
    if (selfCalls == 0) {
      return;
    }
    models::Finding finding;
    finding.id = nextId();
    finding.category = models::FindingCategory::Recursion;
    finding.pattern = selfCalls == 1 ? "direct-recursion" : "multiple-direct-recursion";
    finding.functionName = function.getNameAsString();
    finding.description = "function '" + finding.functionName + "' calls itself at " + std::to_string(selfCalls) +
                          " call site(s); recurrence analysis is not implemented";
    finding.location = location(function.getBeginLoc(), function.getEndLoc());
    finding.boundStatus = BoundStatus::Unsupported;
    finding.confidence = Confidence::None;
    finding.provenance = kRecursionProvenance;
    finding.limitations.push_back("time and stack depth of recursive functions are not estimated yet (milestone M2+)");
    out_.findings.push_back(std::move(finding));
  }

  clang::ASTContext& ctx_;
  std::string displayPath_;
  analyzers::SourceAnalysis& out_;
  std::string currentFunction_;
  int findingCounter_ = 0;
};

class AnalysisConsumer final : public clang::ASTConsumer {
 public:
  AnalysisConsumer(std::string displayPath, analyzers::SourceAnalysis& out)
      : displayPath_(std::move(displayPath)), out_(out) {}

  void HandleTranslationUnit(clang::ASTContext& ctx) override {
    if (ctx.getDiagnostics().hasErrorOccurred()) {
      return;  // Never analyse an AST with errors.
    }
    AnalysisVisitor visitor(ctx, displayPath_, out_);
    visitor.TraverseDecl(ctx.getTranslationUnitDecl());
    std::stable_sort(out_.findings.begin(), out_.findings.end(), [](const auto& a, const auto& b) {
      return std::tie(a.location.line, a.location.column) < std::tie(b.location.line, b.location.column);
    });
    // Identifiers follow source order so they are stable across analyzer changes.
    int next = 0;
    for (auto& finding : out_.findings) {
      finding.id = "F" + std::to_string(++next);
    }
  }

 private:
  std::string displayPath_;
  analyzers::SourceAnalysis& out_;
};

class AnalysisAction final : public clang::ASTFrontendAction {
 public:
  AnalysisAction(std::string displayPath, analyzers::SourceAnalysis& out)
      : displayPath_(std::move(displayPath)), out_(out) {}

 protected:
  std::unique_ptr<clang::ASTConsumer> CreateASTConsumer(clang::CompilerInstance& /*ci*/,
                                                        llvm::StringRef /*file*/) override {
    return std::make_unique<AnalysisConsumer>(displayPath_, out_);
  }

 private:
  std::string displayPath_;
  analyzers::SourceAnalysis& out_;
};

// ---------------------------------------------------------------------------
// SourceAnalyzer implementation
// ---------------------------------------------------------------------------

class ClangSourceAnalyzer final : public analyzers::SourceAnalyzer {
 public:
  explicit ClangSourceAnalyzer(ClangFrontendOptions options) : options_(std::move(options)) {
    options_.resourceDir = resolveResourceDir(options_);
  }

  analyzers::SourceAnalysis analyze(const analyzers::SourceInput& input) override {
    analyzers::SourceAnalysis analysis;

    llvm::IntrusiveRefCntPtr<llvm::vfs::OverlayFileSystem> overlay(
        new llvm::vfs::OverlayFileSystem(llvm::vfs::getRealFileSystem()));
    llvm::IntrusiveRefCntPtr<llvm::vfs::InMemoryFileSystem> memory(new llvm::vfs::InMemoryFileSystem);
    overlay->pushOverlay(memory);
    memory->addFile(kVirtualFileName, 0, llvm::MemoryBuffer::getMemBufferCopy(input.content, kVirtualFileName));
    llvm::IntrusiveRefCntPtr<clang::FileManager> files(new clang::FileManager(clang::FileSystemOptions(), overlay));

    // Arguments are fixed by the engine; nothing from the request is passed
    // through except the validated language standard.
    std::vector<std::string> args = {"bigolens-clang", "-fsyntax-only", "-xc++", "-std=" + input.cppStandard, "-w"};
    if (!options_.resourceDir.empty()) {
      args.push_back("-resource-dir=" + options_.resourceDir);
    }
    args.emplace_back(kVirtualFileName);

    CollectingDiagnostics diagnostics(input.displayPath);
    clang::tooling::ToolInvocation invocation(
        std::move(args), std::make_unique<AnalysisAction>(input.displayPath, analysis), files.get());
    invocation.setDiagnosticConsumer(&diagnostics);
    const bool ok = invocation.run();

    analysis.parsed = ok && diagnostics.getNumErrors() == 0;
    analysis.diagnostics = diagnostics.take();
    if (!analysis.parsed) {
      analysis.findings.clear();
      analysis.storage.clear();
      if (analysis.diagnostics.empty()) {
        analysis.diagnostics.push_back({models::Severity::Error, "the Clang frontend failed to run", std::nullopt});
      }
    }
    return analysis;
  }

  std::string frontendVersion() const override { return clang::getClangFullVersion(); }

 private:
  ClangFrontendOptions options_;
};

}  // namespace

std::string resolveResourceDir(const ClangFrontendOptions& options) {
  if (!options.resourceDir.empty()) {
    return options.resourceDir;
  }
  if (const char* env = std::getenv("BIGOLENS_CLANG_RESOURCE_DIR"); env != nullptr && *env != '\0') {
    return env;
  }
  if (!options.bundledResourceDir.empty()) {
    std::error_code error;
    const std::filesystem::path marker = std::filesystem::path(options.bundledResourceDir) / "include" / "stddef.h";
    if (std::filesystem::is_regular_file(marker, error)) {
      return options.bundledResourceDir;
    }
  }
  return BIGOLENS_DEFAULT_CLANG_RESOURCE_DIR;
}

std::unique_ptr<analyzers::SourceAnalyzer> makeClangSourceAnalyzer(ClangFrontendOptions options) {
  return std::make_unique<ClangSourceAnalyzer>(std::move(options));
}

}  // namespace bigolens::parser
