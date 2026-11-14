// 14_set_insertion_loop.cpp
// Pattern: std::set insert in loop
// Expected: O(n log n), Proven (or Estimated)

namespace std {
template <typename T>
class set {
 public:
  void insert(const T&);
};
}  // namespace std

void set_insert_loop(int n) {
  std::set<int> s;
  for (int i = 0; i < n; ++i) {
    s.insert(i);
  }
}
