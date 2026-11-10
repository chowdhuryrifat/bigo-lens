// L2: for (T i = a; i <= b; ++i).
long long le_from_one(int n) {
  long long s = 0;
  for (int i = 1; i <= n; ++i) {
    s += i;
  }
  return s;
}

// Near miss: increasing index with a condition that does not bound it from above.
long long near_miss_wrong_direction(int n) {
  long long s = 0;
  for (int i = 0; i >= n; ++i) {
    s += i;
  }
  return s;
}
