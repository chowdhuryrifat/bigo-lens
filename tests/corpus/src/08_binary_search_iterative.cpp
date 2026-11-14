// 08_binary_search_iterative.cpp
// Pattern: Iterative binary search (data-dependent loop)
// Expected: Unknown, None (Honesty of results)

int sorted_arr[200005];

int binary_search_iterative(int n, int key) {
  int lo = 0;
  int hi = n - 1;
  while (lo <= hi) {
    int mid = lo + (hi - lo) / 2;
    if (sorted_arr[mid] == key) {
      return mid;
    }
    if (sorted_arr[mid] < key) {
      lo = mid + 1;
    } else {
      hi = mid - 1;
    }
  }
  return -1;
}
