// 09_binary_search_recursive.cpp
// Pattern: Recursive binary search
// Expected: O(log n), Proven

int recursive_binary_search(const int* a, int lo, int hi, int x) {
  if (lo >= hi) {
    return lo;
  }
  const int mid = lo + (hi - lo) / 2;
  if (a[mid] < x) {
    return recursive_binary_search(a, mid + 1, hi, x);
  }
  return recursive_binary_search(a, lo, mid, x);
}
