class Shape {
  Shape(this.name);
  final String name;
}

class Circle extends Shape {
  Circle(double r) : super('circle'), r = r;
  final double r = 0.0;
}

class Point {
  Point(this.x, this.y);
  Point.origin() : this(0, 0);
  final int x;
  final int y;
  int sum() => x + y;
}

enum Color {
  red,
  green,
  blue;
  String label() => name;
}

void main() {
  var p = Point(3, 4);
  print(p.sum());

  var o = Point.origin();
  print(o.x);
  print(o.y);

  var c = Circle(2.5);
  print(c.name);
  print(c.r);

  print(Color.red.label());
  print(Color.values.length);

  var record = (a: 1, 2);
  print(record.a);
  print(record.$0);

  Iterable<int> gen() sync* {
    yield 1;
    yield 2;
    yield* [3, 4];
  }
  for (var v in gen()) {
    print(v);
  }

  var square = (int x) => x * x;
  print(square(7));

  var map = {'a': 10};
  map['b'] = 20;
  print(map['a'] + map['b']);
}
