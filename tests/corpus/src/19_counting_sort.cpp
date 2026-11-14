// 19_counting_sort.cpp
// Pattern: Sequential linear loops
// Expected: O(k + n), Proven

int cnt[100005];
int out[200005];

void counting_sort(const int* input, int n, int k) {
  for (int i = 0; i < k; ++i) {
    cnt[i] = 0;
  }
  for (int i = 0; i < n; ++i) {
    cnt[input[i]]++;
  }
  int idx = 0;
  for (int i = 0; i < k; ++i) {
    for (int j = 0; j < cnt[i]; ++j) {
      out[idx++] = i;
    }
  }
}
