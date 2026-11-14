// 05_grid_traversal.cpp
// Pattern: 2D nested loop
// Expected: O(m n), Proven

int grid[1005][1005];

long long sum_grid(int n, int m) {
  long long total = 0;
  for (int r = 0; r < n; ++r) {
    for (int c = 0; c < m; ++c) {
      total += grid[r][c];
    }
  }
  return total;
}
