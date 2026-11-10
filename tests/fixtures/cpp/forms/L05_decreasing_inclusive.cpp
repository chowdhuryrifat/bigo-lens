// L5: for (T i = a; i >= b; --i) and i -= c.
long long ge_decrement(int n) {
  long long s = 0;
  for (int i = n; i >= 0; --i) {
    s += i;
  }
  return s;
}

long long ge_stride(int n) {
  long long s = 0;
  for (int i = n; i >= 1; i -= 2) {
    s += i;
  }
  return s;
}

// Near miss: an unsigned index compared with >= 0 never terminates.
long long near_miss_unsigned_ge_zero(unsigned n) {
  long long s = 0;
  for (unsigned i = n; i >= 0; --i) {
    s += i;
  }
  return s;
}
