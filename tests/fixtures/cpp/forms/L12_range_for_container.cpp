// L12: range-based for over a named container (any type with begin/end).
struct Values {
  int* first;
  int* last;
  int* begin() const { return first; }
  int* end() const { return last; }
};

Values make_values();

long long over_named_container(const Values& v) {
  long long s = 0;
  for (int x : v) {
    s += x;
  }
  return s;
}

// Near miss: ranging over a function result is not a named container.
long long near_miss_temporary_range() {
  long long s = 0;
  for (int x : make_values()) {
    s += x;
  }
  return s;
}

// Near miss: an element value used as an inner bound is data, not a size.
long long near_miss_element_as_bound(const Values& v) {
  long long s = 0;
  for (int x : v) {
    for (int j = 0; j < x; ++j) {
      s += j;
    }
  }
  return s;
}
