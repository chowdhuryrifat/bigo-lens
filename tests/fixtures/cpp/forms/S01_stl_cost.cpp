// S01: STL operations cost model coverage (BL-205).
// Fixtures for recognized STL calls in loop nests and top level.

namespace std {
template <typename T>
class set {
 public:
  void insert(const T&);
};

template <typename K, typename V>
class map {
 public:
  void* find(const K&) const;
  V& operator[](const K&);
};

template <typename K, typename V>
class unordered_map {
 public:
  void* find(const K&) const;
  V& operator[](const K&);
};

template <typename T>
class priority_queue {
 public:
  void push(const T&);
  void pop();
  const T& top() const;
};

template <typename T>
class vector {
 public:
  vector() = default;
  explicit vector(int);
  void push_back(const T&);
  void erase(void*);
  T* begin() const;
  T* end() const;
};

template <typename It, typename T>
It lower_bound(It first, It last, const T& val);

template <typename It>
void sort(It first, It last);
}  // namespace std

void set_insert(int n) {
  std::set<int> s;
  for (int i = 0; i < n; ++i) {
    s.insert(i);
  }
}

void map_lookup(const std::map<int, int>& m, int n) {
  for (int i = 0; i < n; ++i) {
    m.find(i);
  }
}

void priority_queue_push(int n) {
  std::priority_queue<int> pq;
  for (int i = 0; i < n; ++i) {
    pq.push(i);
  }
}

void vector_push_back(int n) {
  std::vector<int> v;
  for (int i = 0; i < n; ++i) {
    v.push_back(i);
  }
}

void vector_erase(int n, int m) {
  std::vector<int> v(n);
  for (int i = 0; i < m; ++i) {
    v.erase(nullptr);
  }
}

void unordered_map_find(const std::unordered_map<int, int>& u, int n) {
  for (int i = 0; i < n; ++i) {
    u.find(i);
  }
}

void lower_bound_call(const std::vector<int>& v, int m) {
  for (int i = 0; i < m; ++i) {
    std::lower_bound(v.begin(), v.end(), i);
  }
}

void sort_in_loop(int m, int n) {
  std::vector<int> v(n);
  for (int i = 0; i < m; ++i) {
    std::sort(v.begin(), v.end());
  }
}

void top_level_sort(int n) {
  std::vector<int> v(n);
  std::sort(v.begin(), v.end());
}
