// L9: while (t--) and while (t-- > 0).
long long post_decrement(int t, int n) {
  long long s = 0;
  while (t--) {
    for (int i = 0; i < n; ++i) {
      s += i;
    }
  }
  return s;
}

long long post_decrement_compared(int t) {
  long long s = 0;
  while (t-- > 0) {
    s += t;
  }
  return s;
}

// Near miss: the counter is also written in the body.
long long near_miss_counter_written(int t) {
  long long s = 0;
  while (t--) {
    if (s > 100) {
      t = 0;
    }
    s += t;
  }
  return s;
}

// Near miss: pre-decrement runs t - 1 times and is not the t-- idiom.
long long near_miss_pre_decrement(int t) {
  long long s = 0;
  while (--t) {
    s += t;
  }
  return s;
}
