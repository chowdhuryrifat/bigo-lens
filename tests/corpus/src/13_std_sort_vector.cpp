// 13_std_sort_vector.cpp
// Pattern: std::sort STL algorithm
// Expected: O(n log n), Proven / Estimated

#include <algorithm>
#include <vector>

void sort_container(std::vector<int>& v) {
  std::sort(v.begin(), v.end());
}
