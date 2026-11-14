// 18_sum_constrained_multitest.cpp
// Pattern: Multi-testcase loop under sum constraints
// Expected: O(n^2 t), Proven (via sum-constraint-packing)

int buffer[200005];

void solve_cases(int t, int n) {
  while (t--) {
    for (int i = 0; i < n; ++i) {
      for (int j = 0; j < n; ++j) {
        buffer[i] += j;
      }
    }
  }
}
