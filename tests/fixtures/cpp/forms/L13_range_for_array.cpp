// L13: range-based for over a constant-size array.
long long over_local_array() {
  int a[16] = {};
  long long s = 0;
  for (int x : a) {
    s += x;
  }
  return s;
}

long long over_array_reference(const int (&a)[8]) {
  long long s = 0;
  for (int x : a) {
    s += x;
  }
  return s;
}
