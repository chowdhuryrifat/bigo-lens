// 10_merge_sort.cpp
// Pattern: Recursive merge sort (Divide and Conquer, Master Theorem)
// Expected: O(n log n), Proven

void merge_sort(int* a, int* tmp, int lo, int hi) {
  if (hi - lo <= 1) {
    return;
  }
  const int mid = (lo + hi) / 2;
  merge_sort(a, tmp, lo, mid);
  merge_sort(a, tmp, mid, hi);
  for (int i = lo; i < hi; ++i) {
    tmp[i] = a[i];
  }
}
