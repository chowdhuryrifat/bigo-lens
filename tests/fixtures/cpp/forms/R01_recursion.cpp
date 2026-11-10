// Recurrence extraction for direct recursion (BL-202, COMPLEXITY_MODEL.md §9).

// T(n) = T(n - 1) + O(1)
long long linear_sum(int n) {
  if (n <= 0) {
    return 0;
  }
  return n + linear_sum(n - 1);
}

// T(n) = T(n/2) + O(1)
int halving_depth(int n) {
  return n <= 1 ? 0 : 1 + halving_depth(n / 2);
}

// Two call sites, but a return ends the first path: at most one call per invocation.
// T(n) = at most 1 of {2 T(n/2)} + O(1), where n = hi - lo
int binary_search(const int* a, int lo, int hi, int x) {
  if (lo >= hi) {
    return lo;
  }
  const int mid = lo + (hi - lo) / 2;
  if (a[mid] < x) {
    return binary_search(a, mid + 1, hi, x);
  }
  return binary_search(a, lo, mid, x);
}

// T(n) = 2 T(n/2) + O(n), where n = hi - lo (merge loop inlined).
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

// T(n) = T(n - 1) + T(n - 2) + O(1)
long long fibonacci(int n) {
  return n < 2 ? n : fibonacci(n - 1) + fibonacci(n - 2);
}

// Counting up towards a limit: the interval limit - n shrinks by one per call.
// T(n) = T(n - 1) + O(1), where n = limit - n
int count_up(int n, int limit) {
  return n >= limit ? n : count_up(n + 1, limit);
}

// Near miss: the argument grows without a second parameter bounding it.
int near_miss_growing(int n) {
  return n > 1000000 ? n : near_miss_growing(n * 2 + 1);
}

// Near miss: self-calls inside a loop (DFS shape).
void near_miss_dfs(const int* next, int count, int node, bool* seen) {
  seen[node] = true;
  for (int k = 0; k < count; ++k) {
    if (!seen[next[k]]) {
      near_miss_dfs(next, count, next[k], seen);
    }
  }
}

// Near miss: T(n/k) with a variable divisor is not a constant split.
int near_miss_variable_divisor(int n, int k) {
  return n <= 1 ? 0 : 1 + near_miss_variable_divisor(n / k, k);
}
