// Interprocedural analysis fixtures (BL-204).

// Case 1: Helper with a loop.
void helper_loop(int m) {
  for (int i = 0; i < m; ++i) {
  }
}

// Caller loop over n calling O(m) helper yields O(m n), Proven, High.
void caller_loop(int n, int m) {
  for (int i = 0; i < n; ++i) {
    helper_loop(m);
  }
}

// Case 2: Transitive helper calling another helper.
void helper_chain_inner(int k) {
  for (int i = 0; i < k; ++i) {
  }
}

void helper_chain_outer(int k) {
  helper_chain_inner(k);
}

// Caller loop over n calling chained helper yields O(k n), Proven, High.
void caller_chain(int n, int k) {
  for (int i = 0; i < n; ++i) {
    helper_chain_outer(k);
  }
}

// Case 3: Helper with an unknown loop makes caller Unknown.
void helper_unknown(int m) {
  for (int i = 0;; ++i) {
    if (i >= m) {
      break;
    }
  }
}

// Caller loop calling helper with unknown loop becomes Unknown.
void caller_unknown(int n, int m) {
  for (int i = 0; i < n; ++i) {
    helper_unknown(m);
  }
}

// Case 4: Recursive helper falls back to Estimated.
int helper_recursive(int x) {
  if (x <= 1) {
    return 1;
  }
  return helper_recursive(x - 1);
}

// Caller loop calling recursive helper: Estimated, Low.
void caller_recursive(int n) {
  for (int i = 0; i < n; ++i) {
    helper_recursive(n);
  }
}

// Case 5: Call cycle falls back to Estimated.
int helper_cycle_b(int x);

int helper_cycle_a(int x) {
  return helper_cycle_b(x);
}

int helper_cycle_b(int x) {
  return helper_cycle_a(x);
}

// Caller loop calling cycle helper: Estimated, Low.
void caller_cycle(int n) {
  for (int i = 0; i < n; ++i) {
    helper_cycle_a(n);
  }
}

// Case 6: Triangular summation via index argument.
void helper_index(int m) {
  for (int j = 0; j < m; ++j) {
  }
}

// Caller passing loop index i: triangular summation O(n^2), Proven, High.
void caller_index(int n) {
  for (int i = 0; i < n; ++i) {
    helper_index(i);
  }
}
