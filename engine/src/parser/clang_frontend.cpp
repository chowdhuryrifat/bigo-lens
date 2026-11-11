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
#include <clang/Lex/Lexer.h>
#include <clang/Tooling/Tooling.h>
#include <llvm/ADT/SmallString.h>
#include <llvm/Support/MemoryBuffer.h>
#include <llvm/Support/VirtualFileSystem.h>

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <filesystem>
#include <functional>
#include <map>
#include <optional>
#include <set>
#include <string>
#include <tuple>
#include <utility>
#include <vector>

#include "bigolens/complexity/asymptotic.hpp"
#include "bigolens/complexity/evaluator.hpp"
#include "bigolens/complexity/expression.hpp"
#include "bigolens/complexity/recurrence.hpp"
#include "bigolens/estimation/stl_cost_model.hpp"

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

const clang::Expr* stripExpr(const clang::Expr* expr) {
  if (expr == nullptr) {
    return nullptr;
  }
  const clang::Expr* e = expr->IgnoreParenCasts();
  while (e != nullptr) {
    if (const auto* clean = llvm::dyn_cast<clang::ExprWithCleanups>(e)) {
      e = clean->getSubExpr()->IgnoreParenCasts();
    } else if (const auto* bind = llvm::dyn_cast<clang::CXXBindTemporaryExpr>(e)) {
      e = bind->getSubExpr()->IgnoreParenCasts();
    } else if (const auto* mat = llvm::dyn_cast<clang::MaterializeTemporaryExpr>(e)) {
      e = mat->getSubExpr()->IgnoreParenCasts();
    } else if (const auto* cast = llvm::dyn_cast<clang::CastExpr>(e)) {
      e = cast->getSubExpr()->IgnoreParenCasts();
    } else {
      break;
    }
  }
  return e;
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

/// Calls reachable from `stmt` without passing through a loop.
void collectChildCalls(const clang::Stmt* stmt, std::vector<const clang::CallExpr*>& out) {
  if (stmt == nullptr) {
    return;
  }
  if (const auto* call = llvm::dyn_cast<clang::CallExpr>(stmt)) {
    out.push_back(call);
  }
  for (const clang::Stmt* child : stmt->children()) {
    if (child == nullptr || isLoop(child)) {
      continue;
    }
    collectChildCalls(child, out);
  }
}

/// True if `function` contains direct self-calls anywhere in its body.
bool isDirectlyRecursive(const clang::FunctionDecl& function) {
  const clang::FunctionDecl* canonical = function.getCanonicalDecl();
  const clang::Stmt* body = function.getBody();
  if (body == nullptr) {
    return false;
  }
  std::vector<const clang::Stmt*> stack{body};
  while (!stack.empty()) {
    const clang::Stmt* current = stack.back();
    stack.pop_back();
    if (current == nullptr) {
      continue;
    }
    if (const auto* call = llvm::dyn_cast<clang::CallExpr>(current)) {
      if (const clang::FunctionDecl* callee = call->getDirectCallee()) {
        if (callee->getCanonicalDecl() == canonical) {
          return true;
        }
      }
    }
    for (const clang::Stmt* child : current->children()) {
      if (child != nullptr) {
        stack.push_back(child);
      }
    }
  }
  return false;
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

struct FunctionSummary {
  cx::ExprPtr cost = nullptr;
  BoundStatus status = BoundStatus::Proven;
  int depth = 0;
  bool isRecursive = false;
  bool hasCycle = false;
  std::vector<std::string> paramNames;
  std::vector<std::string> assumptions;
  std::vector<std::string> limitations;
  std::string unknownReason;
};

class InterproceduralContext {
 public:
  explicit InterproceduralContext(clang::ASTContext& ctx) : ctx_(ctx) {}

  FunctionSummary summarize(const clang::FunctionDecl* function);

 private:
  clang::ASTContext& ctx_;
  std::map<const clang::FunctionDecl*, FunctionSummary> cache_;
  std::vector<const clang::FunctionDecl*> activeCallStack_;
  static constexpr std::size_t kMaxDepth = 8;
};

class LoopAnalyzer {
 public:
  explicit LoopAnalyzer(clang::ASTContext& ctx, InterproceduralContext* interprocedural = nullptr,
                        const clang::Stmt* functionBody = nullptr)
      : ctx_(ctx), interprocedural_(interprocedural), functionBody_(functionBody) {}

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

 public:
  cx::ExprPtr determineSize(const clang::Expr* objectExpr, const clang::Expr* rangeStart, const clang::Expr* rangeEnd) {
    if (rangeStart != nullptr && rangeEnd != nullptr) {
      const clang::Expr* strippedStart = stripExpr(rangeStart);
      const clang::Expr* strippedEnd = stripExpr(rangeEnd);
      if (const auto* memCall = llvm::dyn_cast_or_null<clang::CXXMemberCallExpr>(strippedStart)) {
        const auto* method = memCall->getMethodDecl();
        const std::string memName = method ? method->getNameAsString() : std::string();
        if (memName == "begin" || memName == "cbegin" || memName == "rbegin") {
          return determineSize(stripExpr(memCall->getImplicitObjectArgument()), nullptr, nullptr);
        }
      } else if (const auto* beginCall = llvm::dyn_cast_or_null<clang::CallExpr>(strippedStart)) {
        if (beginCall->getNumArgs() >= 1) {
          const auto* bCallee = beginCall->getDirectCallee();
          if (bCallee && bCallee->getNameAsString() == "begin") {
            return determineSize(stripExpr(beginCall->getArg(0)), nullptr, nullptr);
          }
        }
      }
      if (const auto* binary = llvm::dyn_cast_or_null<clang::BinaryOperator>(strippedEnd)) {
        if (binary->getOpcode() == clang::BO_Add) {
          const auto* lhs = stripExpr(binary->getLHS());
          if (referencedVar(lhs) != nullptr && referencedVar(lhs) == referencedVar(strippedStart)) {
            if (auto sz = toSymbolic(binary->getRHS())) {
              return sz;
            }
          }
        }
      }
    }

    if (objectExpr != nullptr) {
      const clang::Expr* strippedObj = stripExpr(objectExpr);
      if (const clang::VarDecl* var = referencedVar(strippedObj)) {
        if (const auto* init = var->getInit()) {
          const auto* construct = llvm::dyn_cast<clang::CXXConstructExpr>(init->IgnoreImplicit());
          if (construct != nullptr && construct->getNumArgs() > 0 && construct->getArg(0)->getType()->isIntegerType()) {
            if (auto sz = toSymbolic(construct->getArg(0))) {
              return sz;
            }
          }
        }
        if (llvm::isa<clang::ParmVarDecl>(var)) {
          return cx::variable(var->getNameAsString() + ".size()");
        }
        if (activeLoopTrips_ != nullptr) {
          return activeLoopTrips_;
        }
        const std::string name = var->getNameAsString();
        if (!name.empty()) {
          return cx::variable(name + ".size()");
        }
      }
      if (activeLoopTrips_ != nullptr) {
        return activeLoopTrips_;
      }
    }

    if (activeLoopTrips_ != nullptr) {
      return activeLoopTrips_;
    }
    return cx::variable("n");
  }

  bool analyzeStlCall(const clang::CallExpr* call, NestResult& result, std::vector<cx::ExprPtr>& terms) {
    const clang::FunctionDecl* callee = call->getDirectCallee();
    if (callee == nullptr) {
      return false;
    }
    std::string container;
    std::string operation;
    const clang::Expr* objectExpr = nullptr;
    const clang::Expr* rangeStart = nullptr;
    const clang::Expr* rangeEnd = nullptr;

    if (const auto* member = llvm::dyn_cast<clang::CXXMemberCallExpr>(call)) {
      container = normaliseStdName(qualifiedRecordName(member->getObjectType()));
      operation = callee->getNameAsString();
      objectExpr = member->getImplicitObjectArgument();
    } else if (const auto* opCall = llvm::dyn_cast<clang::CXXOperatorCallExpr>(call)) {
      if (callee->getNameAsString() == "operator[]" && opCall->getNumArgs() >= 2) {
        container = normaliseStdName(qualifiedRecordName(opCall->getArg(0)->getType()));
        operation = "operator[]";
        objectExpr = opCall->getArg(0);
      }
    } else {
      const std::string name = normaliseStdName(callee->getQualifiedNameAsString());
      if (name == "std::sort" || name == "sort") {
        container = "algorithm";
        operation = "sort";
        if (call->getNumArgs() >= 2) {
          rangeStart = call->getArg(0);
          rangeEnd = call->getArg(1);
        }
      } else if (name == "std::lower_bound" || name == "lower_bound") {
        container = "algorithm";
        operation = "lower_bound";
        if (call->getNumArgs() >= 2) {
          rangeStart = call->getArg(0);
          rangeEnd = call->getArg(1);
        }
      }
    }

    if (container.empty() || operation.empty()) {
      return false;
    }

    const auto stlCost = estimation::lookupStlCost(container, operation);
    if (!stlCost.has_value()) {
      return false;
    }

    handledCalls_.insert(call);
    if (rangeStart != nullptr) {
      if (const auto* startCall = llvm::dyn_cast<clang::CallExpr>(stripExpr(rangeStart))) {
        handledCalls_.insert(startCall);
      }
    }
    if (rangeEnd != nullptr) {
      if (const auto* endCall = llvm::dyn_cast<clang::CallExpr>(stripExpr(rangeEnd))) {
        handledCalls_.insert(endCall);
      }
    }

    cx::ExprPtr sizeExpr = determineSize(objectExpr, rangeStart, rangeEnd);
    if (!sizeExpr) {
      sizeExpr = cx::variable("n");
    }

    if (container == "std::unordered_map") {
      result.status = models::weakest(result.status, BoundStatus::Estimated);
      appendUnique(result.assumptions, {stlCost->note,
                                        "std::unordered_map operations assume average-case O(1) performance (worst "
                                        "case is linear due to hash collisions)"});
      result.limitations.push_back("worst-case time for std::unordered_map is O(" + complexity::toString(*sizeExpr) +
                                   ") under hash collisions");
    } else if (stlCost->amortized) {
      appendUnique(result.assumptions, {stlCost->note});
    } else {
      appendUnique(result.assumptions, {stlCost->note});
      cx::ExprPtr opCost = cx::substitute(stlCost->worst, "n", sizeExpr);
      if (opCost && !(opCost->kind() == cx::ExprKind::Constant && opCost->number() == 1.0)) {
        terms.push_back(opCost);
      }
    }
    return true;
  }

  /// Cost of a loop body: the sum of its child loops and called functions, or 1 for a straight-line body.
  NestResult analyzeBody(const clang::Stmt* body) {
    NestResult result;
    std::vector<const clang::Stmt*> children;
    if (body != nullptr && isLoop(body)) {
      children.push_back(body);  // Unbraced body that is itself a loop.
    } else {
      collectChildLoops(body, children);
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
    if (children.size() > 1 && !terms.empty()) {
      result.assumptions.push_back("sequential inner loops are summed");
    }

    if (children.empty() || !terms.empty()) {
      std::vector<const clang::CallExpr*> childCalls;
      if (body != nullptr && !isLoop(body)) {
        collectChildCalls(body, childCalls);
      }
      for (const clang::CallExpr* call : childCalls) {
        const clang::FunctionDecl* callee = call->getDirectCallee();
        if (callee == nullptr) {
          continue;
        }
        const clang::FunctionDecl* def = callee->getDefinition();
        const bool userDefined =
            def != nullptr && ctx_.getSourceManager().isInMainFile(def->getLocation()) && !callee->isImplicit();
        if (!userDefined) {
          analyzeStlCall(call, result, terms);
          continue;
        }
        if (interprocedural_ == nullptr) {
          continue;
        }

        handledCalls_.insert(call);
        FunctionSummary summary = interprocedural_->summarize(def);
        if (summary.isRecursive) {
          result.status = models::weakest(result.status, BoundStatus::Estimated);
          const std::string reason = summary.hasCycle ? "call cycle detected" : "recursive functions are not inlined";
          result.limitations.push_back("calls user function '" + callee->getNameAsString() +
                                       "'; its cost is not included (" + reason + ")");
          continue;
        }

        if (summary.status == BoundStatus::Unknown || !summary.cost) {
          result.fail(BoundStatus::Unknown, summary.unknownReason.empty()
                                                ? ("call to '" + callee->getNameAsString() + "' has unknown cost")
                                                : summary.unknownReason);
          appendUnique(result.limitations, summary.limitations);
          appendUnique(result.assumptions, summary.assumptions);
          terms.clear();
          return result;
        }

        if (summary.status == BoundStatus::Unsupported) {
          result.fail(BoundStatus::Unsupported, summary.unknownReason.empty()
                                                    ? ("call to '" + callee->getNameAsString() + "' is unsupported")
                                                    : summary.unknownReason);
          appendUnique(result.limitations, summary.limitations);
          appendUnique(result.assumptions, summary.assumptions);
          terms.clear();
          return result;
        }

        // Callee has a valid symbolic cost.
        cx::ExprPtr substitutedCost = summary.cost;
        bool mappingOk = true;
        std::string unmappedParam;

        const auto free = cx::freeVariables(*summary.cost);
        std::vector<std::pair<std::string, cx::ExprPtr>> replacements;
        for (std::size_t i = 0; i < summary.paramNames.size(); ++i) {
          const std::string& paramName = summary.paramNames[i];
          if (paramName.empty() || free.count(paramName) == 0) {
            continue;
          }
          if (i >= call->getNumArgs()) {
            mappingOk = false;
            unmappedParam = paramName;
            break;
          }
          const clang::Expr* arg = call->getArg(static_cast<unsigned int>(i));
          cx::ExprPtr argExpr = toSymbolic(arg);
          if (!argExpr) {
            mappingOk = false;
            unmappedParam = paramName;
            break;
          }
          replacements.push_back({paramName, argExpr});
        }

        if (!mappingOk) {
          result.fail(BoundStatus::Unknown, "argument for parameter '" + unmappedParam + "' of '" +
                                                callee->getNameAsString() + "' cannot be determined symbolically");
          terms.clear();
          return result;
        }

        // Simultaneous substitution using temporary symbols.
        for (std::size_t i = 0; i < replacements.size(); ++i) {
          substitutedCost = cx::substitute(substitutedCost, replacements[i].first,
                                           cx::variable("__bigo_temp_arg_" + std::to_string(i) + "__"));
        }
        for (std::size_t i = 0; i < replacements.size(); ++i) {
          substitutedCost =
              cx::substitute(substitutedCost, "__bigo_temp_arg_" + std::to_string(i) + "__", replacements[i].second);
        }

        result.status = models::weakest(result.status, summary.status);
        result.depth = std::max(result.depth, summary.depth);
        appendUnique(result.assumptions, summary.assumptions);
        appendUnique(result.limitations, summary.limitations);
        if (substitutedCost &&
            !(substitutedCost->kind() == cx::ExprKind::Constant && substitutedCost->number() == 1.0)) {
          terms.push_back(substitutedCost);
        }
      }
    }

    if (terms.empty()) {
      if (result.status != BoundStatus::Unknown && result.status != BoundStatus::Unsupported) {
        result.cost = cx::constant(1.0);
      }
    } else {
      result.cost = terms.size() == 1 ? terms.front() : cx::add(std::move(terms));
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
    const auto prevTrips = activeLoopTrips_;
    activeLoopTrips_ = trips;
    NestResult result = analyzeBody(body);
    activeLoopTrips_ = prevTrips;
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

  struct TwoPointerPathState {
    int updates = 0;
    bool invalid = false;
    bool exited = false;
  };

  enum class TwoPointerUpdateKind { None, IncL, DecR, Invalid };

  TwoPointerUpdateKind classifyTwoPointerUpdate(const clang::Expr* expr, const clang::VarDecl* lVar,
                                                const clang::VarDecl* rVar) const {
    if (expr == nullptr) {
      return TwoPointerUpdateKind::None;
    }
    const clang::Expr* stripped = stripExpr(expr);

    if (const auto* unary = llvm::dyn_cast<clang::UnaryOperator>(stripped)) {
      if (unary->isIncrementOp()) {
        if (referencedVar(unary->getSubExpr()) == lVar) {
          return TwoPointerUpdateKind::IncL;
        }
        if (referencedVar(unary->getSubExpr()) == rVar) {
          return TwoPointerUpdateKind::Invalid;
        }
      } else if (unary->isDecrementOp()) {
        if (referencedVar(unary->getSubExpr()) == rVar) {
          return TwoPointerUpdateKind::DecR;
        }
        if (referencedVar(unary->getSubExpr()) == lVar) {
          return TwoPointerUpdateKind::Invalid;
        }
      }
    }

    if (const auto* compound = llvm::dyn_cast<clang::CompoundAssignOperator>(stripped)) {
      const auto rhsVal = constantValue(compound->getRHS(), ctx_);
      if (compound->getOpcode() == clang::BO_AddAssign && referencedVar(compound->getLHS()) == lVar) {
        if (rhsVal && *rhsVal == 1) {
          return TwoPointerUpdateKind::IncL;
        }
        return TwoPointerUpdateKind::Invalid;
      }
      if (compound->getOpcode() == clang::BO_SubAssign && referencedVar(compound->getLHS()) == rVar) {
        if (rhsVal && *rhsVal == 1) {
          return TwoPointerUpdateKind::DecR;
        }
        return TwoPointerUpdateKind::Invalid;
      }
      if (referencedVar(compound->getLHS()) == lVar || referencedVar(compound->getLHS()) == rVar) {
        return TwoPointerUpdateKind::Invalid;
      }
    }

    if (const auto* assign = llvm::dyn_cast<clang::BinaryOperator>(stripped)) {
      if (assign->getOpcode() == clang::BO_Assign) {
        if (referencedVar(assign->getLHS()) == lVar) {
          if (const auto* add = llvm::dyn_cast<clang::BinaryOperator>(assign->getRHS()->IgnoreParenImpCasts())) {
            if (add->getOpcode() == clang::BO_Add) {
              const auto lhsVal = constantValue(add->getLHS(), ctx_);
              const auto rhsVal = constantValue(add->getRHS(), ctx_);
              if (referencedVar(add->getLHS()) == lVar && rhsVal && *rhsVal == 1) {
                return TwoPointerUpdateKind::IncL;
              }
              if (referencedVar(add->getRHS()) == lVar && lhsVal && *lhsVal == 1) {
                return TwoPointerUpdateKind::IncL;
              }
            }
          }
          return TwoPointerUpdateKind::Invalid;
        }
        if (referencedVar(assign->getLHS()) == rVar) {
          if (const auto* sub = llvm::dyn_cast<clang::BinaryOperator>(assign->getRHS()->IgnoreParenImpCasts())) {
            if (sub->getOpcode() == clang::BO_Sub) {
              const auto rhsVal = constantValue(sub->getRHS(), ctx_);
              if (referencedVar(sub->getLHS()) == rVar && rhsVal && *rhsVal == 1) {
                return TwoPointerUpdateKind::DecR;
              }
            }
          }
          return TwoPointerUpdateKind::Invalid;
        }
      }
    }

    if (mayModify(expr, lVar) || mayModify(expr, rVar)) {
      return TwoPointerUpdateKind::Invalid;
    }

    return TwoPointerUpdateKind::None;
  }

  void analyzeTwoPointerPaths(const clang::Stmt* stmt, std::vector<TwoPointerPathState>& inPaths,
                              const clang::VarDecl* lVar, const clang::VarDecl* rVar) const {
    if (stmt == nullptr || inPaths.empty()) {
      return;
    }

    if (const auto* compound = llvm::dyn_cast<clang::CompoundStmt>(stmt)) {
      for (const clang::Stmt* child : compound->body()) {
        analyzeTwoPointerPaths(child, inPaths, lVar, rVar);
      }
      return;
    }

    if (const auto* ifStmt = llvm::dyn_cast<clang::IfStmt>(stmt)) {
      if (mayModify(ifStmt->getCond(), lVar) || mayModify(ifStmt->getCond(), rVar)) {
        for (auto& p : inPaths) {
          if (!p.exited) {
            p.invalid = true;
          }
        }
        return;
      }
      std::vector<TwoPointerPathState> activePaths;
      std::vector<TwoPointerPathState> exitedPaths;
      for (const auto& p : inPaths) {
        if (p.exited || p.invalid) {
          exitedPaths.push_back(p);
        } else {
          activePaths.push_back(p);
        }
      }
      if (activePaths.empty()) {
        return;
      }
      std::vector<TwoPointerPathState> thenPaths = activePaths;
      analyzeTwoPointerPaths(ifStmt->getThen(), thenPaths, lVar, rVar);

      std::vector<TwoPointerPathState> elsePaths = activePaths;
      if (ifStmt->getElse() != nullptr) {
        analyzeTwoPointerPaths(ifStmt->getElse(), elsePaths, lVar, rVar);
      }
      inPaths = std::move(thenPaths);
      inPaths.insert(inPaths.end(), elsePaths.begin(), elsePaths.end());
      inPaths.insert(inPaths.end(), exitedPaths.begin(), exitedPaths.end());
      return;
    }

    if (const auto* condOp = llvm::dyn_cast<clang::ConditionalOperator>(stmt)) {
      if (mayModify(condOp->getCond(), lVar) || mayModify(condOp->getCond(), rVar)) {
        for (auto& p : inPaths) {
          if (!p.exited) {
            p.invalid = true;
          }
        }
        return;
      }
      std::vector<TwoPointerPathState> activePaths;
      std::vector<TwoPointerPathState> exitedPaths;
      for (const auto& p : inPaths) {
        if (p.exited || p.invalid) {
          exitedPaths.push_back(p);
        } else {
          activePaths.push_back(p);
        }
      }
      if (activePaths.empty()) {
        return;
      }
      std::vector<TwoPointerPathState> truePaths = activePaths;
      analyzeTwoPointerPaths(condOp->getTrueExpr(), truePaths, lVar, rVar);

      std::vector<TwoPointerPathState> falsePaths = activePaths;
      analyzeTwoPointerPaths(condOp->getFalseExpr(), falsePaths, lVar, rVar);

      inPaths = std::move(truePaths);
      inPaths.insert(inPaths.end(), falsePaths.begin(), falsePaths.end());
      inPaths.insert(inPaths.end(), exitedPaths.begin(), exitedPaths.end());
      return;
    }

    if (llvm::isa<clang::ReturnStmt, clang::BreakStmt, clang::CXXThrowExpr>(stmt)) {
      for (auto& p : inPaths) {
        if (!p.exited) {
          p.exited = true;
        }
      }
      return;
    }

    if (llvm::isa<clang::ContinueStmt, clang::NullStmt>(stmt)) {
      return;
    }

    if (isLoop(stmt)) {
      if (mayModify(stmt, lVar) || mayModify(stmt, rVar)) {
        for (auto& p : inPaths) {
          if (!p.exited) {
            p.invalid = true;
          }
        }
      }
      return;
    }

    if (const auto* declStmt = llvm::dyn_cast<clang::DeclStmt>(stmt)) {
      if (mayModify(declStmt, lVar) || mayModify(declStmt, rVar)) {
        for (auto& p : inPaths) {
          if (!p.exited) {
            p.invalid = true;
          }
        }
      }
      return;
    }

    if (const auto* expr = llvm::dyn_cast<clang::Expr>(stmt)) {
      TwoPointerUpdateKind kind = classifyTwoPointerUpdate(expr, lVar, rVar);
      for (auto& p : inPaths) {
        if (p.exited || p.invalid) {
          continue;
        }
        if (kind == TwoPointerUpdateKind::IncL || kind == TwoPointerUpdateKind::DecR) {
          p.updates += 1;
        } else if (kind == TwoPointerUpdateKind::Invalid) {
          p.invalid = true;
        }
      }
      return;
    }

    if (mayModify(stmt, lVar) || mayModify(stmt, rVar)) {
      for (auto& p : inPaths) {
        if (!p.exited) {
          p.invalid = true;
        }
      }
    }
  }

  bool mayModifyOutsideLoop(const clang::Stmt* stmt, const clang::Stmt* loop, const clang::VarDecl* var) const {
    if (stmt == nullptr || stmt == loop) {
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
    } else if (llvm::isa<clang::CallExpr>(stmt)) {
      if (mayModify(stmt, var)) {
        return true;
      }
    }
    for (const clang::Stmt* child : stmt->children()) {
      if (mayModifyOutsideLoop(child, loop, var)) {
        return true;
      }
    }
    return false;
  }

  const clang::CompoundStmt* findEnclosingCompound(const clang::Stmt* root, const clang::Stmt* target) const {
    if (root == nullptr) {
      return nullptr;
    }
    if (const auto* compound = llvm::dyn_cast<clang::CompoundStmt>(root)) {
      for (const clang::Stmt* child : compound->body()) {
        if (child == target) {
          return compound;
        }
        if (const auto* found = findEnclosingCompound(child, target)) {
          return found;
        }
      }
    }
    for (const clang::Stmt* child : root->children()) {
      if (const auto* found = findEnclosingCompound(child, target)) {
        return found;
      }
    }
    return nullptr;
  }

  cx::ExprPtr resolveInitialValue(const clang::VarDecl* var, const clang::Stmt* loop) {
    if (var == nullptr) {
      return nullptr;
    }

    if (functionBody_ != nullptr) {
      const auto* compound = findEnclosingCompound(functionBody_, loop);
      if (compound != nullptr) {
        auto it = compound->body_begin();
        auto end = compound->body_end();
        while (it != end && *it != loop) {
          ++it;
        }
        if (it != end) {
          auto rev = it;
          while (rev != compound->body_begin()) {
            --rev;
            const clang::Stmt* stmt = *rev;
            if (const auto* declStmt = llvm::dyn_cast<clang::DeclStmt>(stmt)) {
              for (const clang::Decl* decl : declStmt->decls()) {
                if (decl == var) {
                  if (var->getInit() != nullptr) {
                    return toSymbolic(var->getInit());
                  }
                  return nullptr;
                }
              }
            }
            if (const auto* expr = llvm::dyn_cast<clang::Expr>(stmt)) {
              const clang::Expr* stripped = stripExpr(expr);
              if (const auto* assign = llvm::dyn_cast<clang::BinaryOperator>(stripped)) {
                if (assign->getOpcode() == clang::BO_Assign && referencedVar(assign->getLHS()) == var) {
                  return toSymbolic(assign->getRHS());
                }
              }
            }
            if (mayModify(stmt, var)) {
              return nullptr;
            }
          }
        }
      }
    }

    if (llvm::isa<clang::ParmVarDecl>(var)) {
      if (functionBody_ != nullptr && mayModifyOutsideLoop(functionBody_, loop, var)) {
        return nullptr;
      }
      return cx::variable(var->getNameAsString());
    }

    if (var->hasLocalStorage() && var->getInit() != nullptr) {
      if (functionBody_ != nullptr && mayModifyOutsideLoop(functionBody_, loop, var)) {
        return nullptr;
      }
      return toSymbolic(var->getInit());
    }

    return nullptr;
  }

  std::optional<NestResult> analyzeTwoPointerWhile(const clang::WhileStmt& loop, const clang::VarDecl* lVar,
                                                   const clang::VarDecl* rVar, bool inclusive) {
    std::vector<TwoPointerPathState> paths = {TwoPointerPathState{}};
    analyzeTwoPointerPaths(loop.getBody(), paths, lVar, rVar);
    if (paths.empty()) {
      return std::nullopt;
    }
    int fallthroughCount = 0;
    for (const auto& p : paths) {
      if (p.invalid) {
        return std::nullopt;
      }
      if (!p.exited) {
        ++fallthroughCount;
        if (p.updates != 1) {
          return std::nullopt;
        }
      }
    }
    if (fallthroughCount == 0) {
      return std::nullopt;
    }

    cx::ExprPtr l0 = resolveInitialValue(lVar, &loop);
    cx::ExprPtr r0 = resolveInitialValue(rVar, &loop);
    if (!l0 || !r0) {
      return std::nullopt;
    }

    cx::ExprPtr diff = cx::subtract(r0, l0);
    cx::ExprPtr trips = inclusive ? cx::add({diff, cx::constant(1.0)}) : diff;

    NestResult result = combine(loop.getBody(), std::nullopt, nullptr, nullptr, trips, false);
    appendUnique(result.assumptions,
                 {"two-pointer convergence: at most " + complexity::toString(*trips) + " iterations"});
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

    // 3. Two-pointer while loops: while (l < r), while (r > l), while (l <= r), while (r >= l).
    if (const auto* cmp = llvm::dyn_cast_or_null<clang::BinaryOperator>(cond)) {
      const clang::Expr* lExpr = nullptr;
      const clang::Expr* rExpr = nullptr;
      bool inclusive = false;
      switch (cmp->getOpcode()) {
        case clang::BO_LT:
          lExpr = cmp->getLHS();
          rExpr = cmp->getRHS();
          break;
        case clang::BO_GT:
          lExpr = cmp->getRHS();
          rExpr = cmp->getLHS();
          break;
        case clang::BO_LE:
          lExpr = cmp->getLHS();
          rExpr = cmp->getRHS();
          inclusive = true;
          break;
        case clang::BO_GE:
          lExpr = cmp->getRHS();
          rExpr = cmp->getLHS();
          inclusive = true;
          break;
        default:
          break;
      }
      if (lExpr != nullptr && rExpr != nullptr) {
        const clang::VarDecl* lVar = referencedVar(lExpr);
        const clang::VarDecl* rVar = referencedVar(rExpr);
        if (lVar != nullptr && rVar != nullptr && lVar != rVar && lVar->getType()->isIntegerType() &&
            rVar->getType()->isIntegerType() && !mayModify(cond, lVar) && !mayModify(cond, rVar)) {
          if (auto twoPointerResult = analyzeTwoPointerWhile(loop, lVar, rVar, inclusive)) {
            return *twoPointerResult;
          }
        }
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
            if (interprocedural_ == nullptr || handledCalls_.count(call) == 0) {
              userCalls.insert(callee->getNameAsString());
            }
          } else if (!llvm::isa<clang::CXXOperatorCallExpr>(call) && !llvm::isa<clang::CXXConstructorDecl>(callee)) {
            if (handledCalls_.count(call) == 0) {
              libraryCalls.insert(describeLibraryCall(*call, *callee));
            }
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
  InterproceduralContext* interprocedural_ = nullptr;
  const clang::Stmt* functionBody_ = nullptr;
  std::vector<LoopIndex> indices_;
  std::set<const clang::VarDecl*> dataVars_;
  std::set<const clang::CallExpr*> handledCalls_;
  cx::ExprPtr activeLoopTrips_ = nullptr;
};

FunctionSummary InterproceduralContext::summarize(const clang::FunctionDecl* function) {
  if (function == nullptr) {
    return {};
  }
  const clang::FunctionDecl* canonical = function->getCanonicalDecl();
  const clang::FunctionDecl* def = function->getDefinition();
  if (def == nullptr || !ctx_.getSourceManager().isInMainFile(def->getLocation()) || def->isImplicit()) {
    return {};
  }

  auto it = cache_.find(canonical);
  if (it != cache_.end()) {
    return it->second;
  }

  if (std::find(activeCallStack_.begin(), activeCallStack_.end(), canonical) != activeCallStack_.end()) {
    FunctionSummary cycleSummary;
    cycleSummary.isRecursive = true;
    cycleSummary.hasCycle = true;
    cycleSummary.status = BoundStatus::Estimated;
    return cycleSummary;
  }

  if (activeCallStack_.size() >= kMaxDepth) {
    FunctionSummary depthSummary;
    depthSummary.status = BoundStatus::Estimated;
    depthSummary.limitations.push_back("calls user function '" + def->getNameAsString() +
                                       "'; its cost is not included (call depth limit exceeded)");
    return depthSummary;
  }

  if (isDirectlyRecursive(*def)) {
    FunctionSummary recSummary;
    recSummary.isRecursive = true;
    recSummary.status = BoundStatus::Estimated;
    cache_[canonical] = recSummary;
    return recSummary;
  }

  activeCallStack_.push_back(canonical);

  FunctionSummary summary;
  for (const clang::ParmVarDecl* param : def->parameters()) {
    summary.paramNames.push_back(param ? param->getNameAsString() : "");
  }

  LoopAnalyzer analyzer(ctx_, this, def->getBody());
  NestResult bodyResult = analyzer.analyzeBody(def->getBody());

  summary.cost = bodyResult.cost;
  summary.status = bodyResult.status;
  summary.depth = bodyResult.depth;
  summary.assumptions = std::move(bodyResult.assumptions);
  summary.limitations = std::move(bodyResult.limitations);
  summary.unknownReason = std::move(bodyResult.unknownReason);

  activeCallStack_.pop_back();

  cache_[canonical] = summary;
  return summary;
}

// ---------------------------------------------------------------------------
// Recurrence extraction (direct recursion)
// ---------------------------------------------------------------------------

struct SelfCallSite {
  const clang::CallExpr* call;
  bool insideLoop;
};

/// Extracts T(n) = sum T(call sizes) + work(n) for a directly recursive
/// function. Only the shapes documented in COMPLEXITY_MODEL.md §9 are
/// classified; everything else is left as SizeChange::Unknown.
class RecurrenceExtractor {
 public:
  RecurrenceExtractor(clang::ASTContext& ctx, const clang::FunctionDecl& function)
      : ctx_(ctx), function_(function), canonical_(function.getCanonicalDecl()) {}

  [[nodiscard]] std::vector<SelfCallSite> selfCalls() const {
    std::vector<SelfCallSite> sites;
    collect(function_.getBody(), false, sites);
    return sites;
  }

  [[nodiscard]] cx::Recurrence extract(const std::vector<SelfCallSite>& sites,
                                       const std::function<models::SourceLocation(const clang::Stmt&)>& where) {
    cx::Recurrence recurrence;
    recurrence.function = function_.getNameAsString();
    recurrence.maxCallsPerInvocation = pathCalls(function_.getBody());
    for (const auto& site : sites) {
      cx::RecursiveCall call;
      call.insideLoop = site.insideLoop;
      call.argument = argumentsAsWritten(*site.call);
      call.location = where(*site.call);
      recurrence.calls.push_back(std::move(call));
    }

    if (!classifySingleParameter(sites, recurrence) && !classifyInterval(sites, recurrence)) {
      for (auto& call : recurrence.calls) {
        call.change = cx::SizeChange::Unknown;
      }
    }
    if (usesIntegerDivision_) {
      recurrence.assumptions.push_back("integer division in call arguments is treated as real division");
    }
    computeWork(recurrence);
    return recurrence;
  }

 private:
  bool isSelfCall(const clang::Stmt* stmt) const {
    const auto* call = llvm::dyn_cast_or_null<clang::CallExpr>(stmt);
    const clang::FunctionDecl* callee = call ? call->getDirectCallee() : nullptr;
    return callee != nullptr && callee->getCanonicalDecl() == canonical_;
  }

  void collect(const clang::Stmt* stmt, bool inLoop, std::vector<SelfCallSite>& out) const {
    if (stmt == nullptr) {
      return;
    }
    if (isSelfCall(stmt)) {
      out.push_back({llvm::cast<clang::CallExpr>(stmt), inLoop});
    }
    const bool loop = inLoop || isLoop(stmt);
    for (const clang::Stmt* child : stmt->children()) {
      collect(child, loop, out);
    }
  }

  std::size_t countSelfCalls(const clang::Stmt* stmt) const {
    std::vector<SelfCallSite> sites;
    collect(stmt, false, sites);
    return sites.size();
  }

  /// Self-calls along the paths through a statement: the most calls on a path
  /// that falls through to the next statement and on a path that returns.
  /// kNever marks "no such path"; `unknown` is set when a loop contains a call.
  struct PathCalls {
    static constexpr int kNever = -1;
    int fall = 0;
    int exit = kNever;
    bool unknown = false;
  };

  static int plus(int a, int b) { return a == PathCalls::kNever || b == PathCalls::kNever ? PathCalls::kNever : a + b; }

  /// Statements in sequence: a return in an earlier statement ends the path.
  static PathCalls sequence(const PathCalls& first, const PathCalls& second) {
    PathCalls out;
    out.unknown = first.unknown || second.unknown;
    out.fall = plus(first.fall, second.fall);
    out.exit = std::max(first.exit, plus(first.fall, second.exit));
    return out;
  }

  /// Exclusive alternatives after a shared head (if/else, ?:).
  static PathCalls alternatives(const PathCalls& head, const PathCalls& a, const PathCalls& b) {
    PathCalls choice;
    choice.unknown = a.unknown || b.unknown;
    choice.fall = std::max(a.fall, b.fall);
    choice.exit = std::max(a.exit, b.exit);
    return sequence(head, choice);
  }

  PathCalls paths(const clang::Stmt* stmt) const {
    PathCalls out;
    if (stmt == nullptr) {
      return out;
    }
    if (isLoop(stmt)) {
      out.unknown = countSelfCalls(stmt) != 0;
      return out;
    }
    if (const auto* ifStmt = llvm::dyn_cast<clang::IfStmt>(stmt)) {
      PathCalls head = sequence(paths(ifStmt->getInit()), paths(ifStmt->getConditionVariableDeclStmt()));
      head = sequence(head, paths(ifStmt->getCond()));
      return alternatives(head, paths(ifStmt->getThen()), paths(ifStmt->getElse()));
    }
    if (const auto* conditional = llvm::dyn_cast<clang::ConditionalOperator>(stmt)) {
      return alternatives(paths(conditional->getCond()), paths(conditional->getTrueExpr()),
                          paths(conditional->getFalseExpr()));
    }
    if (const auto* ret = llvm::dyn_cast<clang::ReturnStmt>(stmt)) {
      const PathCalls value = paths(ret->getRetValue());
      out.unknown = value.unknown;
      out.fall = PathCalls::kNever;
      out.exit = std::max(value.fall, value.exit);
      return out;
    }
    // Children evaluated in order (compound statements, expressions).
    for (const clang::Stmt* child : stmt->children()) {
      out = sequence(out, paths(child));
    }
    if (isSelfCall(stmt)) {
      out = sequence(out, PathCalls{1, PathCalls::kNever, false});
    }
    return out;
  }

  /// Largest number of self-calls on any single path; empty if a loop contains one.
  std::optional<int> pathCalls(const clang::Stmt* stmt) const {
    const PathCalls result = paths(stmt);
    if (result.unknown) {
      return std::nullopt;
    }
    return std::max({0, result.fall, result.exit});
  }

  std::string argumentsAsWritten(const clang::CallExpr& call) const {
    const auto& sm = ctx_.getSourceManager();
    std::string out;
    for (unsigned i = 0; i < call.getNumArgs(); ++i) {
      const clang::SourceRange range = call.getArg(i)->getSourceRange();
      const auto text =
          clang::Lexer::getSourceText(clang::CharSourceRange::getTokenRange(range), sm, ctx_.getLangOpts());
      out += (i == 0 ? "" : ", ") + text.str();
    }
    return out;
  }

  bool isOwnParameter(const clang::VarDecl* var) const {
    const auto* param = llvm::dyn_cast_or_null<clang::ParmVarDecl>(var);
    return param != nullptr && param->getDeclContext() == &function_;
  }

  /// Integer expression over the function's parameters; locals with an
  /// initializer that are never written are substituted (e.g. mid).
  cx::ExprPtr toExpr(const clang::Expr* expr, int substitutions = 0) {
    if (expr == nullptr || substitutions > 4) {
      return nullptr;
    }
    if (auto value = constantValue(expr, ctx_)) {
      return cx::constant(static_cast<double>(*value));
    }
    const clang::Expr* e = expr->IgnoreParenCasts();
    if (const clang::VarDecl* var = referencedVar(e)) {
      if (!var->getType()->isIntegerType()) {
        return nullptr;
      }
      if (isOwnParameter(var)) {
        return cx::variable(var->getNameAsString());
      }
      if (var->hasLocalStorage() && var->getInit() != nullptr && !mayModify(function_.getBody(), var)) {
        return toExpr(var->getInit(), substitutions + 1);
      }
      return nullptr;
    }
    if (const auto* binary = llvm::dyn_cast<clang::BinaryOperator>(e)) {
      cx::ExprPtr lhs = toExpr(binary->getLHS(), substitutions);
      cx::ExprPtr rhs = toExpr(binary->getRHS(), substitutions);
      if (!lhs || !rhs) {
        return nullptr;
      }
      const bool rhsConstant = rhs->kind() == cx::ExprKind::Constant;
      switch (binary->getOpcode()) {
        case clang::BO_Add:
          return cx::add({lhs, rhs});
        case clang::BO_Sub:
          return cx::subtract(lhs, rhs);
        case clang::BO_Mul:
          return cx::multiply({lhs, rhs});
        case clang::BO_Div:
          if (rhsConstant && rhs->number() > 0.0) {
            usesIntegerDivision_ = true;
            return cx::multiply({lhs, cx::constant(1.0 / rhs->number())});
          }
          return nullptr;
        case clang::BO_Shr:
          if (rhsConstant && rhs->number() >= 1.0 && rhs->number() <= 30.0) {
            usesIntegerDivision_ = true;
            return cx::multiply({lhs, cx::constant(1.0 / std::ldexp(1.0, static_cast<int>(rhs->number())))});
          }
          return nullptr;
        default:
          return nullptr;
      }
    }
    return nullptr;
  }

  /// value(expr) with the given bindings, or nullopt.
  static std::optional<double> at(const cx::ExprPtr& expr, const cx::Bindings& bindings) {
    const auto result = cx::evaluate(*expr, bindings);
    return result.ok() ? std::optional<double>(result.value) : std::nullopt;
  }

  /// True if expr is at most linear (the converter emits only +, -, constant
  /// multiples and products; the degree check excludes products of variables).
  static bool isLinear(const cx::ExprPtr& expr) {
    const auto bound = cx::asymptoticUpperBound(*expr);
    if (!bound) {
      return false;
    }
    for (const auto& term : bound->terms) {
      double degree = 0.0;
      for (const auto& [name, factor] : term) {
        degree += factor.power;
        if (factor.logPower != 0.0) {
          return false;
        }
      }
      if (degree > 1.0) {
        return false;
      }
    }
    return true;
  }

  /// newSize = beta * n + gamma (exact for linear forms) -> change.
  static void classify(double beta, double gamma, cx::RecursiveCall& call) {
    constexpr double kEps = 1e-9;
    call.change = cx::SizeChange::Unknown;
    if (std::fabs(beta - 1.0) < kEps && gamma <= -1.0 + kEps) {
      call.change = cx::SizeChange::Decrement;
      call.amount = -gamma;
    } else if (beta > kEps && beta <= 0.5 + kEps && gamma <= 1.0 + kEps) {
      const double b = 1.0 / beta;
      if (std::fabs(b - std::round(b)) < 1e-6) {
        call.change = cx::SizeChange::Divide;
        call.amount = std::round(b);
      }
    }
  }

  bool allClassified(const cx::Recurrence& recurrence) const {
    return std::all_of(recurrence.calls.begin(), recurrence.calls.end(),
                       [](const cx::RecursiveCall& c) { return c.change != cx::SizeChange::Unknown; });
  }

  /// One integer parameter p with every call argument at that position linear
  /// in p alone and shrinking: p - c or p / b.
  bool classifySingleParameter(const std::vector<SelfCallSite>& sites, cx::Recurrence& recurrence) {
    for (unsigned k = 0; k < function_.getNumParams(); ++k) {
      const clang::ParmVarDecl* param = function_.getParamDecl(k);
      if (!param->getType()->isIntegerType() || param->getName().empty()) {
        continue;
      }
      const std::string name = param->getNameAsString();
      bool ok = true;
      for (std::size_t i = 0; i < sites.size() && ok; ++i) {
        const clang::CallExpr& call = *sites[i].call;
        if (k >= call.getNumArgs()) {
          ok = false;
          break;
        }
        cx::ExprPtr arg = toExpr(call.getArg(k));
        if (!arg || !isLinear(arg)) {
          ok = false;
          break;
        }
        const auto free = cx::freeVariables(*arg);
        if (!(free.empty() || (free.size() == 1 && free.count(name) == 1))) {
          ok = false;
          break;
        }
        const auto v0 = at(arg, {{name, 0.0}});
        const auto v1 = at(arg, {{name, 1.0}});
        if (!v0 || !v1) {
          ok = false;
          break;
        }
        classify(*v1 - *v0, *v0, recurrence.calls[i]);
        ok = recurrence.calls[i].change != cx::SizeChange::Unknown;
      }
      if (ok && !sites.empty()) {
        recurrence.size = name;
        sizeParams_ = {name};
        return true;
      }
    }
    return false;
  }

  /// Two integer parameters (lo, hi) whose difference shrinks at every call:
  /// the new difference is linear and translation invariant, beta*(hi-lo)+gamma.
  bool classifyInterval(const std::vector<SelfCallSite>& sites, cx::Recurrence& recurrence) {
    const unsigned params = function_.getNumParams();
    for (unsigned i = 0; i < params; ++i) {
      for (unsigned j = 0; j < params; ++j) {
        if (i == j || !function_.getParamDecl(i)->getType()->isIntegerType() ||
            !function_.getParamDecl(j)->getType()->isIntegerType()) {
          continue;
        }
        const std::string lo = function_.getParamDecl(i)->getNameAsString();
        const std::string hi = function_.getParamDecl(j)->getNameAsString();
        if (lo.empty() || hi.empty()) {
          continue;
        }
        bool ok = true;
        for (std::size_t c = 0; c < sites.size() && ok; ++c) {
          const clang::CallExpr& call = *sites[c].call;
          if (std::max(i, j) >= call.getNumArgs()) {
            ok = false;
            break;
          }
          cx::ExprPtr newLo = toExpr(call.getArg(i));
          cx::ExprPtr newHi = toExpr(call.getArg(j));
          if (!newLo || !newHi) {
            ok = false;
            break;
          }
          cx::ExprPtr size = cx::subtract(newHi, newLo);
          const auto free = cx::freeVariables(*size);
          if (!isLinear(size) ||
              std::any_of(free.begin(), free.end(), [&](const std::string& v) { return v != lo && v != hi; })) {
            ok = false;
            break;
          }
          const auto g = at(size, {{lo, 0.0}, {hi, 0.0}});
          const auto a = at(size, {{lo, 1.0}, {hi, 0.0}});
          const auto b = at(size, {{lo, 0.0}, {hi, 1.0}});
          if (!g || !a || !b) {
            ok = false;
            break;
          }
          const double alpha = *a - *g;
          const double beta = *b - *g;
          if (std::fabs(alpha + beta) > 1e-9) {  // depends on the interval position
            ok = false;
            break;
          }
          classify(beta, *g, recurrence.calls[c]);
          ok = recurrence.calls[c].change != cx::SizeChange::Unknown;
        }
        if (ok && !sites.empty()) {
          recurrence.size = hi + " - " + lo;
          sizeParams_ = {lo, hi};
          return true;
        }
      }
    }
    return false;
  }

  /// Non-recursive structural cost: the loop nests of the body (1 if none),
  /// rewritten in terms of the size variable n.
  void computeWork(cx::Recurrence& recurrence) {
    std::vector<const clang::Stmt*> loops;
    collectChildLoops(function_.getBody(), loops);
    std::vector<cx::ExprPtr> terms;
    for (const clang::Stmt* loop : loops) {
      if (countSelfCalls(loop) != 0) {
        recurrence.workLimitations.push_back("self-calls inside loops are not modelled; f(n) is unknown");
        recurrence.work = nullptr;
        return;
      }
      LoopAnalyzer analyzer(ctx_, nullptr, function_.getBody());
      NestResult nest = analyzer.analyzeNest(loop);
      if (!nest.cost) {
        recurrence.workLimitations.push_back("a loop in the body has an unknown iteration count; f(n) is unknown");
        recurrence.work = nullptr;
        return;
      }
      appendUnique(recurrence.workLimitations, nest.limitations);
      terms.push_back(nest.cost);
    }
    cx::ExprPtr work = terms.empty() ? cx::constant(1.0) : cx::add(std::move(terms));

    if (sizeParams_.size() == 1) {
      work = cx::substitute(work, sizeParams_[0], cx::variable("n"));
    } else if (sizeParams_.size() == 2) {
      const std::string& lo = sizeParams_[0];
      const std::string& hi = sizeParams_[1];
      cx::ExprPtr shifted = cx::substitute(cx::substitute(work, lo, cx::constant(0.0)), hi, cx::variable("n"));
      // f must depend on the interval only through its length. Checked at
      // sample points when f has no other free variables.
      const auto free = cx::freeVariables(*work);
      const bool onlyInterval =
          std::all_of(free.begin(), free.end(), [&](const std::string& v) { return v == lo || v == hi; });
      for (const auto& [l, h] : std::vector<std::pair<double, double>>{{3, 103}, {1000, 5000}, {17, 17}}) {
        if (!onlyInterval) {
          break;
        }
        const auto original = at(work, {{lo, l}, {hi, h}});
        const auto rewritten = at(shifted, {{"n", h - l}});
        if (!original || !rewritten || std::fabs(*original - *rewritten) > 1e-6 * std::max(1.0, *original)) {
          recurrence.workLimitations.push_back(
              "the non-recursive work depends on the interval position; f(n) is unknown");
          recurrence.work = nullptr;
          return;
        }
      }
      work = shifted;
    }
    recurrence.work = work;

    // Calls to other user functions are not included in f(n).
    std::set<std::string> userCalls;
    std::vector<const clang::Stmt*> stack{function_.getBody()};
    while (!stack.empty()) {
      const clang::Stmt* current = stack.back();
      stack.pop_back();
      if (current == nullptr) {
        continue;
      }
      if (const auto* call = llvm::dyn_cast<clang::CallExpr>(current); call != nullptr && !isSelfCall(call)) {
        if (const clang::FunctionDecl* callee = call->getDirectCallee()) {
          const clang::FunctionDecl* definition = callee->getDefinition();
          if (definition != nullptr && ctx_.getSourceManager().isInMainFile(definition->getLocation()) &&
              !callee->isImplicit()) {
            userCalls.insert(callee->getNameAsString());
          }
        }
      }
      for (const clang::Stmt* child : current->children()) {
        stack.push_back(child);
      }
    }
    for (const auto& name : userCalls) {
      recurrence.workLimitations.push_back("calls user function '" + name +
                                           "'; its cost is not included in f(n) (interprocedural analysis is not "
                                           "implemented)");
    }
  }

  clang::ASTContext& ctx_;
  const clang::FunctionDecl& function_;
  const clang::FunctionDecl* canonical_;
  std::vector<std::string> sizeParams_;
  bool usesIntegerDivision_ = false;
};

// ---------------------------------------------------------------------------
// Translation-unit walker
// ---------------------------------------------------------------------------

class AnalysisVisitor final : public clang::RecursiveASTVisitor<AnalysisVisitor> {
 public:
  AnalysisVisitor(clang::ASTContext& ctx, std::string displayPath, analyzers::SourceAnalysis& out)
      : ctx_(ctx), displayPath_(std::move(displayPath)), out_(out), interprocedural_(ctx) {}

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
    analyzeStringConstruction(*var);
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
        if (binary->getOpcode() == clang::BO_Sub) {
          return cx::subtract(lhs, rhs);
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

  void addStorageFinding(const estimation::StorageItem& item, models::FindingCategory category, std::string pattern,
                         cx::ExprPtr bytes, BoundStatus status = BoundStatus::Proven,
                         std::string customDescription = {}, std::vector<std::string> customAssumptions = {},
                         std::vector<std::string> customLimitations = {}) {
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
    if (!customDescription.empty()) {
      finding.description = std::move(customDescription);
    } else if (category == models::FindingCategory::StaticStorage) {
      finding.description = "static-storage array '" + item.name + "' of type " + item.typeName +
                            " (size from the compiler's type layout)";
    } else {
      finding.description = "std::vector '" + item.name + "' constructed with a symbolic element count";
    }

    if (!customAssumptions.empty()) {
      finding.assumptions = std::move(customAssumptions);
    } else if (category != models::FindingCategory::StaticStorage) {
      finding.assumptions.push_back("element buffer only; std::vector bookkeeping and allocator overhead excluded");
    }

    if (!customLimitations.empty()) {
      finding.limitations = std::move(customLimitations);
    } else if (category != models::FindingCategory::StaticStorage) {
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

    // Check if element is also std::vector (2D vector)
    if (normaliseStdName(qualifiedRecordName(element)) == "std::vector" && construct->getNumArgs() >= 2) {
      const auto* innerArg = construct->getArg(1);
      const auto* innerConstruct = llvm::dyn_cast_or_null<clang::CXXConstructExpr>(stripExpr(innerArg));
      if (innerConstruct == nullptr) {
        if (const auto* ref = referencedVar(innerArg)) {
          if (ref->getInit()) {
            innerConstruct = llvm::dyn_cast_or_null<clang::CXXConstructExpr>(stripExpr(ref->getInit()));
          }
        }
      }
      if (innerConstruct != nullptr && innerConstruct->getNumArgs() >= 1 &&
          !innerConstruct->getArg(0)->getType()->isIntegerType()) {
        const auto* sub = llvm::dyn_cast_or_null<clang::CXXConstructExpr>(stripExpr(innerConstruct->getArg(0)));
        if (sub != nullptr && sub->getNumArgs() >= 1 && sub->getArg(0)->getType()->isIntegerType()) {
          innerConstruct = sub;
        }
      }
      if (innerConstruct != nullptr && innerConstruct->getNumArgs() >= 1 &&
          innerConstruct->getArg(0)->getType()->isIntegerType()) {
        cx::ExprPtr innerCount = converter.convert(innerConstruct->getArg(0));
        const auto* innerSpec = llvm::dyn_cast_or_null<clang::ClassTemplateSpecializationDecl>(
            element.getCanonicalType()->getAsCXXRecordDecl());
        if (innerCount && innerSpec != nullptr && innerSpec->getTemplateArgs().size() > 0) {
          const clang::QualType innerElement = innerSpec->getTemplateArgs()[0].getAsType();
          if (!innerElement.isNull() && !innerElement->isDependentType() && !innerElement->isIncompleteType()) {
            const auto innerElementBytes = static_cast<double>(ctx_.getTypeSizeInChars(innerElement).getQuantity());
            estimation::StorageItem item;
            item.name = var.getNameAsString();
            item.kind = estimation::StorageKind::VectorBuffer;
            item.typeName = type.getAsString(ctx_.getPrintingPolicy());
            item.symbolicBytes = cx::add({cx::multiply({count, innerCount, cx::constant(innerElementBytes)}),
                                          cx::multiply({count, cx::constant(elementBytes)})});
            item.location = location(var.getBeginLoc(), var.getEndLoc());
            const std::string description = "std::vector '" + item.name + "' constructed with symbolic element counts";
            const std::vector<std::string> assumptions = {
                "element buffers only; std::vector bookkeeping and allocator overhead excluded"};
            addStorageFinding(item, models::FindingCategory::DynamicAllocation, "sized-vector", item.symbolicBytes,
                              BoundStatus::Estimated, description, assumptions);
            out_.storage.push_back(std::move(item));
            return;
          }
        }
      }
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

  void analyzeStringConstruction(const clang::VarDecl& var) {
    const clang::QualType type = var.getType();
    const std::string recordName = normaliseStdName(qualifiedRecordName(type));
    if (recordName != "std::basic_string" && recordName != "std::string") {
      return;
    }
    const auto* construct =
        llvm::dyn_cast_or_null<clang::CXXConstructExpr>(var.getInit() ? var.getInit()->IgnoreImplicit() : nullptr);
    if (construct == nullptr || construct->getNumArgs() < 2 || !construct->getArg(0)->getType()->isIntegerType()) {
      return;
    }
    LoopAnalyzerSizeConverter converter(ctx_);
    cx::ExprPtr count = converter.convert(construct->getArg(0));
    if (!count) {
      return;
    }
    estimation::StorageItem item;
    item.name = var.getNameAsString();
    item.kind = estimation::StorageKind::VectorBuffer;
    item.typeName = type.getAsString(ctx_.getPrintingPolicy());
    item.symbolicBytes = cx::multiply({count, cx::constant(1.0)});
    item.location = location(var.getBeginLoc(), var.getEndLoc());
    const std::string description = "std::string '" + item.name + "' constructed with a symbolic character count";
    const std::vector<std::string> assumptions = {
        "character buffer only; std::string bookkeeping (SSO) and allocator overhead excluded"};
    addStorageFinding(item, models::FindingCategory::DynamicAllocation, "sized-string", item.symbolicBytes,
                      BoundStatus::Estimated, description, assumptions);
    out_.storage.push_back(std::move(item));
  }

  void analyzeFunction(const clang::FunctionDecl& function) {
    currentFunction_ = function.getNameAsString();
    detectRecursion(function);

    std::vector<const clang::Stmt*> loops;
    collectChildLoops(function.getBody(), loops);
    for (const clang::Stmt* loop : loops) {
      LoopAnalyzer analyzer(ctx_, &interprocedural_, function.getBody());
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

    std::vector<const clang::CallExpr*> topLevelCalls;
    collectChildCalls(function.getBody(), topLevelCalls);
    for (const clang::CallExpr* call : topLevelCalls) {
      const clang::FunctionDecl* callee = call->getDirectCallee();
      if (callee == nullptr) {
        continue;
      }
      const std::string name = normaliseStdName(callee->getQualifiedNameAsString());
      if (name != "std::sort" && name != "sort" && name != "std::lower_bound" && name != "lower_bound") {
        continue;
      }
      const std::string op = (name == "std::sort" || name == "sort") ? "sort" : "lower_bound";
      const auto stlCost = estimation::lookupStlCost("algorithm", op);
      if (!stlCost.has_value()) {
        continue;
      }

      const clang::Expr* rangeStart = call->getNumArgs() >= 1 ? call->getArg(0) : nullptr;
      const clang::Expr* rangeEnd = call->getNumArgs() >= 2 ? call->getArg(1) : nullptr;
      LoopAnalyzer analyzer(ctx_, &interprocedural_, function.getBody());
      cx::ExprPtr sizeExpr = analyzer.determineSize(nullptr, rangeStart, rangeEnd);
      if (!sizeExpr) {
        sizeExpr = cx::variable("n");
      }

      cx::ExprPtr cost = cx::substitute(stlCost->worst, "n", sizeExpr);

      models::Finding finding;
      finding.id = nextId();
      finding.category = models::FindingCategory::LoopNest;
      finding.pattern = "stl-algorithm";
      finding.functionName = currentFunction_;
      finding.location = location(call->getBeginLoc(), call->getEndLoc());
      finding.provenance = kLoopProvenance;
      finding.description = "call to '" + name + "' over range of size " + complexity::toString(*sizeExpr);
      finding.timeExpression = cost;
      finding.boundStatus = BoundStatus::Proven;
      finding.confidence = Confidence::Medium;
      finding.assumptions.push_back(stlCost->note);

      out_.findings.push_back(std::move(finding));
    }
  }

  void detectRecursion(const clang::FunctionDecl& function) {
    RecurrenceExtractor extractor(ctx_, function);
    const std::vector<SelfCallSite> sites = extractor.selfCalls();
    if (sites.empty()) {
      return;
    }
    models::Finding finding;
    finding.id = nextId();
    finding.category = models::FindingCategory::Recursion;
    finding.pattern = sites.size() == 1 ? "direct-recursion" : "multiple-direct-recursion";
    finding.functionName = function.getNameAsString();
    finding.description =
        "function '" + finding.functionName + "' calls itself at " + std::to_string(sites.size()) + " call site(s)";
    finding.location = location(function.getBeginLoc(), function.getEndLoc());
    finding.boundStatus = BoundStatus::Unsupported;
    finding.confidence = Confidence::None;
    finding.provenance = kRecursionProvenance;
    finding.recurrence = extractor.extract(
        sites, [this](const clang::Stmt& stmt) { return location(stmt.getBeginLoc(), stmt.getEndLoc()); });
    out_.findings.push_back(std::move(finding));
  }

  clang::ASTContext& ctx_;
  std::string displayPath_;
  analyzers::SourceAnalysis& out_;
  InterproceduralContext interprocedural_;
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
