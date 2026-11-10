// L11: same conditions as L10 with one update x /= c (c >= 2) or x >>= k.
int halve_divide(int x) {
  int bits = 0;
  while (x > 0) {
    x /= 2;
    ++bits;
  }
  return bits;
}

int halve_shift(int x) {
  int bits = 0;
  while (x >= 1) {
    x >>= 1;
    ++bits;
  }
  return bits;
}

int halve_unbraced(int x) {
  while (x != 0)
    x /= 10;
  return x;
}

// Near miss: dividing by 1 does not shrink x.
int near_miss_divide_by_one(int x) {
  int bits = 0;
  while (x > 0) {
    x /= 1;
    ++bits;
  }
  return bits;
}

// Near miss: plain assignment form is not recognised.
int near_miss_assignment_form(int x) {
  int bits = 0;
  while (x > 0) {
    x = x / 2;
    ++bits;
  }
  return bits;
}
