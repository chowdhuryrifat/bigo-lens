// 15_nested_vectors_dp.cpp
// Pattern: 2D vector allocation and nested DP loop
// Expected: O(m n), Proven time; Estimated memory

#include <vector>

long long solve_grid_dp(int n, int m) {
  std::vector<std::vector<int>> dp(n, std::vector<int>(m));
  for (int i = 0; i < n; ++i) {
    for (int j = 0; j < m; ++j) {
      if (i == 0 || j == 0) {
        dp[i][j] = 1;
      } else {
        dp[i][j] = dp[i - 1][j] + dp[i][j - 1];
      }
    }
  }
  return dp[n - 1][m - 1];
}
