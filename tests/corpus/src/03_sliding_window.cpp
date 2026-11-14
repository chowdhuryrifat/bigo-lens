// 03_sliding_window.cpp
// Pattern: Fixed sliding window
// Expected: O(n), Proven

int arr[200005];

long long max_subarray_sum(int n, int k) {
  long long window_sum = 0;
  for (int i = 0; i < k; ++i) {
    window_sum += arr[i];
  }
  long long max_sum = window_sum;
  for (int i = k; i < n; ++i) {
    window_sum += arr[i] - arr[i - k];
    if (window_sum > max_sum) {
      max_sum = window_sum;
    }
  }
  return max_sum;
}
