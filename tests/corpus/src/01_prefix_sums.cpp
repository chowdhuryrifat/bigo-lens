// 01_prefix_sums.cpp
// Pattern: Linear loop
// Expected: O(n), Proven

int a[1000005];
long long pref[1000005];

void compute_prefix_sums(int n) {
  pref[0] = 0;
  for (int i = 0; i < n; ++i) {
    pref[i + 1] = pref[i] + a[i];
  }
}
