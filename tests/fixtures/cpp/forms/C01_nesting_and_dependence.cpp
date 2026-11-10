// Composition: dependent (triangular) bounds, upper-triangular inner loops and
// sequential inner loops (COMPLEXITY_MODEL.md §2.1).
long long lower_triangle(int n) {
  long long s = 0;
  for (int i = 0; i < n; ++i) {
    for (int j = 0; j < i; ++j) {
      s += j;
    }
  }
  return s;
}

long long upper_triangle(int n) {
  long long s = 0;
  for (int i = 0; i < n; ++i) {
    for (int j = i + 1; j < n; ++j) {
      s += j;
    }
  }
  return s;
}

long long sequential_inner(int n, int m) {
  long long s = 0;
  for (int i = 0; i < n; ++i) {
    for (int j = 0; j < n; ++j) {
      s += j;
    }
    for (int k = 0; k < m; ++k) {
      s -= k;
    }
  }
  return s;
}

long long nested_logarithmic(int n) {
  long long s = 0;
  for (int i = 0; i < n; ++i) {
    for (int j = 1; j < n; j *= 2) {
      s += j;
    }
  }
  return s;
}

// Near miss: the inner bound depends on an index with a non-unit step.
long long near_miss_strided_dependent(int n) {
  long long s = 0;
  for (int i = 0; i < n; i += 2) {
    for (int j = 0; j < i; ++j) {
      s += j;
    }
  }
  return s;
}

// Near miss: one unknown inner loop makes the whole nest unknown.
long long near_miss_unknown_inner(int n, const int* a) {
  long long s = 0;
  for (int i = 0; i < n; ++i) {
    int k = 0;
    while (a[k] != i) {
      ++k;
    }
    s += k;
  }
  return s;
}
