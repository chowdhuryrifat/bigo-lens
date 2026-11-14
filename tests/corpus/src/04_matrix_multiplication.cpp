// 04_matrix_multiplication.cpp
// Pattern: 3-level nested loop
// Expected: O(k m n) or O(n^3), Proven

int A[500][500];
int B[500][500];
long long C[500][500];

void multiply_matrices(int n, int m, int k) {
  for (int i = 0; i < n; ++i) {
    for (int j = 0; j < k; ++j) {
      C[i][j] = 0;
      for (int p = 0; p < m; ++p) {
        C[i][j] += static_cast<long long>(A[i][p]) * B[p][j];
      }
    }
  }
}
