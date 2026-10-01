//! dart:core coverage: num/double/int, BigInt, DateTime, Duration, RegExp,
//! Uri, errors and exceptions.

use d4rt_rs::{Context, Value};

fn expr(src: &str) -> Value {
    Context::new().eval_expression(src).unwrap()
}

// ---------------------------------------------------------------------------
// num / double / int
// ---------------------------------------------------------------------------

#[test]
fn int_methods() {
    assert_eq!(expr("10.isEven"), Value::Bool(true));
    assert_eq!(expr("7.isEven"), Value::Bool(false));
    assert_eq!(expr("10.isOdd"), Value::Bool(false));
    assert_eq!(expr("7.isOdd"), Value::Bool(true));
    assert_eq!(expr("(-7).isNegative"), Value::Bool(true));
    assert_eq!(expr("7.isNegative"), Value::Bool(false));
    assert_eq!(expr("7.sign"), Value::Int(1));
    assert_eq!(expr("(-7).sign"), Value::Int(-1));
    assert_eq!(expr("0.sign"), Value::Int(1));
    assert_eq!(expr("42.bitLength"), Value::Int(6));
    assert_eq!(expr("1.bitLength"), Value::Int(1));
    assert_eq!(expr("0.bitLength"), Value::Int(0));
}

#[test]
fn double_methods() {
    assert_eq!(expr("1.5.isNaN"), Value::Bool(false));
    assert_eq!(expr("(0.0 / 0.0).isNaN"), Value::Bool(true));
    assert_eq!(expr("1.0.isFinite"), Value::Bool(true));
    assert_eq!(expr("(1 / 0).isInfinite"), Value::Bool(true));
    assert_eq!(expr("(1 / 0).isFinite"), Value::Bool(false));
    assert_eq!(expr("(-2.5).sign"), Value::Double(-1.0));
    assert_eq!(expr("2.5.sign"), Value::Double(1.0));
    assert_eq!(expr("(-0.0).isNegative"), Value::Bool(true));
    assert_eq!(expr("3.5.isNegative"), Value::Bool(false));
}

#[test]
fn num_statics() {
    assert_eq!(expr("double.infinity"), Value::Double(f64::INFINITY));
    assert_eq!(
        expr("double.negativeInfinity"),
        Value::Double(f64::NEG_INFINITY)
    );
    assert_eq!(expr("double.maxFinite"), Value::Double(f64::MAX));
    assert_eq!(expr("double.minPositive"), Value::Double(f64::MIN_POSITIVE));
    assert_eq!(expr("int.parse('42')"), Value::Int(42));
    assert_eq!(expr("int.parse('-17')"), Value::Int(-17));
    assert_eq!(expr("int.parse('0x1F')"), Value::Int(31));
    assert_eq!(expr("int.parse(' 42 ')"), Value::Int(42));
    assert_eq!(expr("int.parse('101', radix: 2)"), Value::Int(5));
    assert_eq!(expr("int.parse('ff', radix: 16)"), Value::Int(255));
    assert_eq!(expr("int.tryParse('42')"), Value::Int(42));
    assert_eq!(expr("int.tryParse('abc')"), Value::Null);
    assert_eq!(expr("double.parse('3.5')"), Value::Double(3.5));
    assert_eq!(expr("double.parse('1e3')"), Value::Double(1000.0));
    assert_eq!(expr("double.tryParse('x')"), Value::Null);
    assert_eq!(expr("num.parse('7')"), Value::Int(7));
}

#[test]
fn num_conversions() {
    assert_eq!(expr("3.9.toInt()"), Value::Int(3));
    assert_eq!(expr("(-3.9).toInt()"), Value::Int(-3));
    assert_eq!(expr("7.toDouble()"), Value::Double(7.0));
    assert_eq!(expr("3.14159.toString()"), Value::str("3.14159"));
    assert_eq!(expr("42.toString()"), Value::str("42"));
    assert_eq!(expr("(-5).toString()"), Value::str("-5"));
}

// ---------------------------------------------------------------------------
// BigInt
// ---------------------------------------------------------------------------

#[test]
fn bigint_parse_and_convert() {
    assert_eq!(
        expr("BigInt.parse('12345678901234567890').toString()"),
        Value::str("12345678901234567890")
    );
    assert_eq!(expr("BigInt.parse('-100').toString()"), Value::str("-100"));
    assert_eq!(expr("BigInt.tryParse('abc')"), Value::Null);
    assert_eq!(expr("BigInt.tryParse('42').toString()"), Value::str("42"));
    assert_eq!(expr("BigInt.zero.toString()"), Value::str("0"));
    assert_eq!(expr("BigInt.one.toString()"), Value::str("1"));
    assert_eq!(expr("BigInt.two.toString()"), Value::str("2"));
    assert_eq!(expr("BigInt.from(42).toInt()"), Value::Int(42));
    assert_eq!(expr("BigInt.from(42).toDouble()"), Value::Double(42.0));
    assert_eq!(expr("BigInt.from(255).toRadixString(16)"), Value::str("ff"));
    assert_eq!(expr("BigInt.from(10).toRadixString(2)"), Value::str("1010"));
}

#[test]
fn bigint_arithmetic() {
    assert_eq!(
        expr("(BigInt.from(10) + BigInt.from(5)).toString()"),
        Value::str("15")
    );
    assert_eq!(
        expr("(BigInt.from(10) + BigInt.from(5)).toString()"),
        Value::str("15")
    );
    assert_eq!(
        expr("(BigInt.from(10) - BigInt.from(5)).toString()"),
        Value::str("5")
    );
    assert_eq!(
        expr("(BigInt.from(10) * BigInt.from(5)).toString()"),
        Value::str("50")
    );
    assert_eq!(
        expr("(BigInt.from(10) ~/ BigInt.from(3)).toString()"),
        Value::str("3")
    );
    assert_eq!(
        expr("(BigInt.from(10) % BigInt.from(3)).toString()"),
        Value::str("1")
    );
    assert_eq!(expr("(-BigInt.from(5)).toString()"), Value::str("-5"));
    assert_eq!(expr("(BigInt.from(10) + 5).toString()"), Value::str("15"));
    assert_eq!(expr("(5 + BigInt.from(10)).toString()"), Value::str("15"));
    assert_eq!(
        expr("(BigInt.from(2).pow(10)).toString()"),
        Value::str("1024")
    );
    assert_eq!(
        expr("(BigInt.parse('100').pow(2) == BigInt.from(10000))"),
        Value::Bool(true)
    );
    assert_eq!(
        expr("(BigInt.from(10) > BigInt.from(5))"),
        Value::Bool(true)
    );
    assert_eq!(
        expr("(BigInt.from(10) < BigInt.from(5))"),
        Value::Bool(false)
    );
    assert_eq!(expr("(BigInt.from(10) >= 10)"), Value::Bool(true));
    assert_eq!(expr("(BigInt.from(10) <= 9)"), Value::Bool(false));
    assert_eq!(
        expr("BigInt.from(10).compareTo(BigInt.from(5))"),
        Value::Int(1)
    );
    assert_eq!(expr("BigInt.from(10).compareTo(10)"), Value::Int(0));
}

#[test]
fn bigint_advanced() {
    assert_eq!(
        expr("BigInt.from(12).gcd(BigInt.from(18)).toString()"),
        Value::str("6")
    );
    assert_eq!(expr("BigInt.from(-5).abs().toString()"), Value::str("5"));
    assert_eq!(
        expr("BigInt.from(5).modInverse(BigInt.from(7)).toString()"),
        Value::str("3")
    );
    assert_eq!(
        expr("(BigInt.from(3).modPow(BigInt.from(4), BigInt.from(5))).toString()"),
        Value::str("1")
    );
    assert_eq!(
        expr("BigInt.from(10).remainder(BigInt.from(3)).toString()"),
        Value::str("1")
    );
    assert_eq!(
        expr("(BigInt.from(10) & BigInt.from(3)).toString()"),
        Value::str("2")
    );
    assert_eq!(
        expr("(BigInt.from(10) | BigInt.from(3)).toString()"),
        Value::str("11")
    );
    assert_eq!(
        expr("(BigInt.from(10) ^ BigInt.from(3)).toString()"),
        Value::str("9")
    );
    assert_eq!(
        expr("(BigInt.from(1) << 10).toString()"),
        Value::str("1024")
    );
    assert_eq!(
        expr("(BigInt.from(1024) >> 2).toString()"),
        Value::str("256")
    );
    assert_eq!(expr("(~BigInt.from(0)).toString()"), Value::str("-1"));
    assert_eq!(
        expr("BigInt.from(-1).toUnsigned(8).toString()"),
        Value::str("255")
    );
    assert_eq!(
        expr("BigInt.from(255).toSigned(8).toString()"),
        Value::str("-1")
    );
    assert_eq!(expr("BigInt.from(6).isEven"), Value::Bool(true));
    assert_eq!(expr("BigInt.from(7).isOdd"), Value::Bool(true));
    assert_eq!(expr("BigInt.from(-3).isNegative"), Value::Bool(true));
    assert_eq!(expr("BigInt.from(3).sign"), Value::Int(1));
    assert_eq!(expr("BigInt.from(-3).sign"), Value::Int(-1));
    assert_eq!(expr("BigInt.from(255).bitLength"), Value::Int(8));
    assert_eq!(expr("BigInt.from(42).isValidInt"), Value::Bool(true));
}

// ---------------------------------------------------------------------------
// Duration
// ---------------------------------------------------------------------------

#[test]
fn duration_construction() {
    assert_eq!(expr("Duration(seconds: 90).inMinutes"), Value::Int(1));
    assert_eq!(expr("Duration(seconds: 90).inSeconds"), Value::Int(90));
    assert_eq!(expr("Duration(minutes: 2).inSeconds"), Value::Int(120));
    assert_eq!(expr("Duration(hours: 1).inMinutes"), Value::Int(60));
    assert_eq!(expr("Duration(days: 1).inHours"), Value::Int(24));
    assert_eq!(
        expr("Duration(milliseconds: 1500).inMilliseconds"),
        Value::Int(1500)
    );
    assert_eq!(
        expr("Duration(microseconds: 2000).inMicroseconds"),
        Value::Int(2000)
    );
    assert_eq!(
        expr("Duration(hours: 1, minutes: 30).inMinutes"),
        Value::Int(90)
    );
    assert_eq!(expr("Duration(seconds: -30).isNegative"), Value::Bool(true));
    assert_eq!(expr("Duration(seconds: 30).isNegative"), Value::Bool(false));
    assert_eq!(expr("Duration.zero.inSeconds"), Value::Int(0));
}

#[test]
fn duration_arithmetic() {
    assert_eq!(
        expr("(Duration(seconds: 30) + Duration(seconds: 15)).inSeconds"),
        Value::Int(45)
    );
    assert_eq!(
        expr("(Duration(seconds: 30) - Duration(seconds: 15)).inSeconds"),
        Value::Int(15)
    );
    assert_eq!(
        expr("(Duration(seconds: 10) * 3).inSeconds"),
        Value::Int(30)
    );
    assert_eq!(
        expr("(Duration(seconds: 10) ~/ 4).inSeconds"),
        Value::Int(2)
    );
    assert_eq!(expr("(-Duration(seconds: 10)).inSeconds"), Value::Int(-10));
    assert_eq!(
        expr("Duration(seconds: 10).abs().inSeconds"),
        Value::Int(10)
    );
    assert_eq!(
        expr("Duration(seconds: -10).abs().inSeconds"),
        Value::Int(10)
    );
    assert_eq!(
        expr("Duration(seconds: 10).compareTo(Duration(seconds: 5))"),
        Value::Int(1)
    );
    assert_eq!(
        expr("(Duration(seconds: 10) > Duration(seconds: 5))"),
        Value::Bool(true)
    );
    assert_eq!(
        expr("(Duration(seconds: 10) == Duration(seconds: 10))"),
        Value::Bool(true)
    );
}

#[test]
fn duration_constants() {
    assert_eq!(expr("Duration.microsecondsPerSecond"), Value::Int(1000000));
    assert_eq!(expr("Duration.millisecondsPerSecond"), Value::Int(1000));
    assert_eq!(expr("Duration.secondsPerMinute"), Value::Int(60));
    assert_eq!(expr("Duration.minutesPerHour"), Value::Int(60));
    assert_eq!(expr("Duration.secondsPerHour"), Value::Int(3600));
    assert_eq!(expr("Duration.hoursPerDay"), Value::Int(24));
    assert_eq!(
        expr("Duration(seconds: 3661).toString()"),
        Value::str("1:01:01.000000")
    );
}

// ---------------------------------------------------------------------------
// DateTime
// ---------------------------------------------------------------------------

#[test]
fn datetime_construction() {
    assert_eq!(expr("DateTime(2023, 5, 15).year"), Value::Int(2023));
    assert_eq!(expr("DateTime(2023, 5, 15).month"), Value::Int(5));
    assert_eq!(expr("DateTime(2023, 5, 15).day"), Value::Int(15));
    assert_eq!(
        expr("DateTime(2023, 5, 15, 14, 30, 45).hour"),
        Value::Int(14)
    );
    assert_eq!(
        expr("DateTime(2023, 5, 15, 14, 30, 45).minute"),
        Value::Int(30)
    );
    assert_eq!(
        expr("DateTime(2023, 5, 15, 14, 30, 45).second"),
        Value::Int(45)
    );
    assert_eq!(
        expr("DateTime(2023, 5, 15, 14, 30, 45, 250).millisecond"),
        Value::Int(250)
    );
    assert_eq!(
        expr("DateTime(2023, 5, 15, 14, 30, 45, 250, 100).microsecond"),
        Value::Int(100)
    );
    assert_eq!(expr("DateTime(2023, 5, 15).weekday"), Value::Int(1));
    assert_eq!(expr("DateTime(2024, 2, 29).day"), Value::Int(29));
    assert_eq!(expr("DateTime(2024, 2, 29).isUtc"), Value::Bool(false));
    assert_eq!(expr("DateTime.utc(2024, 1, 1).isUtc"), Value::Bool(true));
    assert_eq!(expr("DateTime.utc(2024, 1, 1).year"), Value::Int(2024));
}

#[test]
fn datetime_parse_and_iso() {
    assert_eq!(expr("DateTime.parse('2024-02-29').day"), Value::Int(29));
    assert_eq!(expr("DateTime.parse('2024-02-29').month"), Value::Int(2));
    assert_eq!(expr("DateTime.parse('2024-02-29').year"), Value::Int(2024));
    assert_eq!(
        expr("DateTime.parse('2024-02-29 12:30:00').hour"),
        Value::Int(12)
    );
    assert_eq!(
        expr("DateTime.parse('2024-02-29T12:30:00').minute"),
        Value::Int(30)
    );
    assert_eq!(
        expr("DateTime.parse('2024-02-29T12:30:00Z').isUtc"),
        Value::Bool(true)
    );
    assert_eq!(expr("DateTime.tryParse('not-a-date')"), Value::Null);
    assert_eq!(
        expr("DateTime.utc(2024, 1, 2, 3, 4, 5).toIso8601String()"),
        Value::str("2024-01-02T03:04:05.000Z")
    );
    assert_eq!(
        expr("DateTime(2024, 1, 2, 3, 4, 5).toIso8601String()"),
        Value::str("2024-01-02T03:04:05.000")
    );
}

#[test]
fn datetime_operations() {
    assert_eq!(
        expr("DateTime(2024, 1, 2).difference(DateTime(2024, 1, 1)).inDays"),
        Value::Int(1)
    );
    assert_eq!(
        expr("DateTime(2024, 1, 1).add(Duration(days: 10)).day"),
        Value::Int(11)
    );
    assert_eq!(
        expr("DateTime(2024, 1, 11).subtract(Duration(days: 10)).day"),
        Value::Int(1)
    );
    assert_eq!(
        expr("DateTime(2024, 1, 2).isAfter(DateTime(2024, 1, 1))"),
        Value::Bool(true)
    );
    assert_eq!(
        expr("DateTime(2024, 1, 1).isBefore(DateTime(2024, 1, 2))"),
        Value::Bool(true)
    );
    assert_eq!(
        expr("DateTime(2024, 1, 1).isAtSameMomentAs(DateTime(2024, 1, 1))"),
        Value::Bool(true)
    );
    assert_eq!(
        expr("DateTime(2024, 1, 1).compareTo(DateTime(2023, 12, 31))"),
        Value::Int(1)
    );
    assert_eq!(
        expr("DateTime.utc(2024, 1, 1).millisecondsSinceEpoch"),
        Value::Int(1704067200000)
    );
    assert_eq!(
        expr("DateTime.utc(2024, 1, 1).microsecondsSinceEpoch"),
        Value::Int(1704067200000000)
    );
    assert_eq!(
        expr("DateTime.fromMillisecondsSinceEpoch(0, isUtc: true).year"),
        Value::Int(1970)
    );
    assert_eq!(
        expr("DateTime.fromMicrosecondsSinceEpoch(0, isUtc: true).year"),
        Value::Int(1970)
    );
    assert_eq!(
        expr("DateTime(2024, 1, 1).toUtc().isUtc"),
        Value::Bool(true)
    );
    assert_eq!(
        expr("DateTime.utc(2024, 1, 1).toLocal().isUtc"),
        Value::Bool(false)
    );
}

// ---------------------------------------------------------------------------
// RegExp
// ---------------------------------------------------------------------------

#[test]
fn regexp_matching() {
    assert_eq!(expr("RegExp(r'\\d+').hasMatch('a1b2')"), Value::Bool(true));
    assert_eq!(expr("RegExp(r'\\d+').hasMatch('abc')"), Value::Bool(false));
    assert_eq!(expr("RegExp(r'a').stringMatch('banana')"), Value::str("a"));
    assert_eq!(
        expr("RegExp(r'\\d+').firstMatch('a1b22c').start"),
        Value::Int(1)
    );
    assert_eq!(
        expr("RegExp(r'\\d+').firstMatch('a1b22c').end"),
        Value::Int(2)
    );
    assert_eq!(
        expr("RegExp(r'\\d+').allMatches('a1b22c').length"),
        Value::Int(2)
    );
    assert_eq!(
        expr("RegExp(r'\\d+').allMatches('a1b22c')[1][0]"),
        Value::str("22")
    );
    assert_eq!(
        expr("RegExp(r'\\d+').matchAsPrefix('12abc').start"),
        Value::Int(0)
    );
    assert_eq!(expr("RegExp(r'\\d+').matchAsPrefix('abc12')"), Value::Null);
    assert_eq!(expr("RegExp(r'\\d+').pattern"), Value::str("\\d+"));
    assert_eq!(expr("RegExp(r'\\d+').isCaseSensitive"), Value::Bool(true));
    assert_eq!(
        expr("RegExp(r'abc', caseSensitive: false).hasMatch('ABC')"),
        Value::Bool(true)
    );
    assert_eq!(expr("RegExp(r'a').groupCount"), Value::Int(0));
}

#[test]
fn regexp_groups() {
    assert_eq!(
        expr("RegExp(r'(\\d+)-(\\d+)').firstMatch('12-34').group(1)"),
        Value::str("12")
    );
    assert_eq!(
        expr("RegExp(r'(\\d+)-(\\d+)').firstMatch('12-34').group(2)"),
        Value::str("34")
    );
    assert_eq!(
        expr("RegExp(r'(\\d+)-(\\d+)').firstMatch('12-34').group(0)"),
        Value::str("12-34")
    );
    assert_eq!(
        expr("RegExp(r'(\\d+)-(\\d+)').firstMatch('12-34').groups([1, 2]).join(',')"),
        Value::str("12,34")
    );
    assert_eq!(
        expr("RegExp(r'(?<year>\\d{4})').firstMatch('2024').namedGroup('year')"),
        Value::str("2024")
    );
    assert_eq!(
        expr("RegExp(r'(?<year>\\d{4})').firstMatch('2024').groupNames.join(',')"),
        Value::str("year")
    );
    assert_eq!(
        expr("RegExp(r'\\d+').firstMatch('a1').input"),
        Value::str("a1")
    );
}

#[test]
fn regexp_statics_and_eq() {
    assert_eq!(expr("RegExp.escape('a.b*')"), Value::str("a\\.b\\*"));
    assert_eq!(expr("(RegExp(r'a') == RegExp(r'a'))"), Value::Bool(true));
    assert_eq!(
        expr("RegExp(r'a', multiLine: true).isMultiLine"),
        Value::Bool(true)
    );
    assert_eq!(
        expr("RegExp(r'a', dotAll: true).isDotAll"),
        Value::Bool(true)
    );
    assert_eq!(
        expr("RegExp(r'a', unicode: true).isUnicode"),
        Value::Bool(true)
    );
    assert_eq!(
        expr("RegExp(r'\\bword\\b').hasMatch('a word here')"),
        Value::Bool(true)
    );
}

// ---------------------------------------------------------------------------
// Uri
// ---------------------------------------------------------------------------

#[test]
fn uri_parts() {
    assert_eq!(
        expr("Uri.parse('https://user:pass@example.com:8080/path/to?q=1#frag').scheme"),
        Value::str("https")
    );
    assert_eq!(
        expr("Uri.parse('https://example.com').host"),
        Value::str("example.com")
    );
    assert_eq!(
        expr("Uri.parse('https://example.com:8080').port"),
        Value::Int(8080)
    );
    assert_eq!(
        expr("Uri.parse('https://example.com/path/to').path"),
        Value::str("/path/to")
    );
    assert_eq!(
        expr("Uri.parse('https://example.com/path/to').pathSegments.join(',')"),
        Value::str("path,to")
    );
    assert_eq!(
        expr("Uri.parse('https://example.com?q=1&r=2').query"),
        Value::str("q=1&r=2")
    );
    assert_eq!(
        expr("Uri.parse('https://example.com#frag').fragment"),
        Value::str("frag")
    );
    assert_eq!(
        expr("Uri.parse('https://u:p@example.com').userInfo"),
        Value::str("u:p")
    );
    assert_eq!(
        expr("Uri.parse('https://u:p@example.com').authority"),
        Value::str("u:p@example.com")
    );
    assert_eq!(
        expr("Uri.parse('https://example.com').origin"),
        Value::str("https://example.com")
    );
    assert_eq!(
        expr("Uri.parse('https://example.com').toString()"),
        Value::str("https://example.com")
    );
}

#[test]
fn uri_flags() {
    assert_eq!(
        expr("Uri.parse('https://example.com').isAbsolute"),
        Value::Bool(true)
    );
    assert_eq!(expr("Uri.parse('/rel').isAbsolute"), Value::Bool(false));
    assert_eq!(
        expr("Uri.parse('https://example.com').hasScheme"),
        Value::Bool(true)
    );
    assert_eq!(
        expr("Uri.parse('https://example.com').hasAuthority"),
        Value::Bool(true)
    );
    assert_eq!(
        expr("Uri.parse('https://example.com').hasQuery"),
        Value::Bool(false)
    );
    assert_eq!(
        expr("Uri.parse('https://example.com?a=1').hasQuery"),
        Value::Bool(true)
    );
    assert_eq!(
        expr("Uri.parse('https://example.com').hasFragment"),
        Value::Bool(false)
    );
    assert_eq!(
        expr("Uri.parse('https://example.com#f').hasFragment"),
        Value::Bool(true)
    );
    assert_eq!(
        expr("Uri.parse('https://example.com').hasPort"),
        Value::Bool(false)
    );
    assert_eq!(
        expr("Uri.parse('https://example.com:80').hasPort"),
        Value::Bool(true)
    );
    assert_eq!(expr("Uri.parse('/a/b').hasAbsolutePath"), Value::Bool(true));
    assert_eq!(expr("Uri.parse('a/b').hasEmptyPath"), Value::Bool(false));
}

#[test]
fn uri_query() {
    assert_eq!(
        expr("Uri.parse('https://x?a=1&b=2').queryParameters['a']"),
        Value::str("1")
    );
    assert_eq!(
        expr("Uri.parse('https://x?a=1&a=2').queryParametersAll['a'].length"),
        Value::Int(2)
    );
    assert_eq!(
        expr("Uri.parse('https://x?a=1&a=2').queryParametersAll['a'][1]"),
        Value::str("2")
    );
    assert_eq!(
        expr("Uri.splitQueryString('a=1&b=2')['b']"),
        Value::str("2")
    );
    assert_eq!(
        expr("Uri.parse('https://x?a%20b=1').queryParameters.keys.first"),
        Value::str("a b")
    );
}

#[test]
fn uri_resolution() {
    assert_eq!(
        expr("Uri.parse('https://x/a/b').resolve('../c').toString()"),
        Value::str("https://x/c")
    );
    assert_eq!(
        expr("Uri.parse('https://x/a/b').resolveUri(Uri.parse('../c')).toString()"),
        Value::str("https://x/c")
    );
    assert_eq!(
        expr("Uri.parse('https://x/a/b').resolve('c/d').toString()"),
        Value::str("https://x/a/c/d")
    );
    assert_eq!(
        expr("Uri.parse('https://x/a/b').removeFragment().toString()"),
        Value::str("https://x/a/b")
    );
    assert_eq!(
        expr("Uri.parse('https://x/a').replace(queryParameters: {'k': 'v'}).toString()"),
        Value::str("https://x/a?k=v")
    );
    assert_eq!(
        expr("Uri.parse('https://x/a').directory"),
        Value::str("https://x/")
    );
    assert_eq!(
        expr("Uri.file('/tmp/x.txt').toFilePath()"),
        Value::str("/tmp/x.txt")
    );
    assert_eq!(
        expr("Uri.parse('/tmp/x').isScheme('file')"),
        Value::Bool(false)
    );
    assert_eq!(
        expr("Uri.parse('file:///tmp/x').isScheme('file')"),
        Value::Bool(true)
    );
}

#[test]
fn uri_encoding() {
    assert_eq!(
        expr("Uri.encodeComponent('a b&c=d')"),
        Value::str("a%20b%26c%3Dd")
    );
    assert_eq!(expr("Uri.decodeComponent('a%20b')"), Value::str("a b"));
    assert_eq!(expr("Uri.encodeFull('a/b?c')"), Value::str("a/b?c"));
    assert_eq!(expr("Uri.decodeFull('a/b%3Fc')"), Value::str("a/b?c"));
    assert_eq!(expr("Uri.encodeQueryComponent('a b')"), Value::str("a+b"));
    assert_eq!(expr("Uri.decodeQueryComponent('a+b')"), Value::str("a b"));
    assert_eq!(expr("Uri.normalizePath('/a/./b/../c')"), Value::str("/a/c"));
    assert_eq!(
        expr("Uri.parseIPv4Address('192.168.1.1').length"),
        Value::Int(4)
    );
    assert_eq!(
        expr("Uri.parseIPv4Address('192.168.1.1')[0]"),
        Value::Int(192)
    );
    assert_eq!(expr("Uri.parseIPv6Address('::1').length"), Value::Int(16));
}

#[test]
fn uri_constructors() {
    assert_eq!(
        expr("Uri.http('example.com', '/x').toString()"),
        Value::str("http://example.com/x")
    );
    assert_eq!(
        expr("Uri.https('example.com', '/x').toString()"),
        Value::str("https://example.com/x")
    );
    assert_eq!(
        expr("Uri.parse('https://x').replace(scheme: 'http').scheme"),
        Value::str("http")
    );
    assert_eq!(expr("Uri.tryParse('https://x').host"), Value::str("x"));
    assert_eq!(expr("Uri.tryParse('::not valid::')"), Value::Null);
}

// ---------------------------------------------------------------------------
// Errors & exceptions
// ---------------------------------------------------------------------------

#[test]
fn argument_error() {
    assert_eq!(
        expr("ArgumentError('msg').toString()"),
        Value::str("Invalid argument(s): msg")
    );
    assert_eq!(
        expr("ArgumentError.value(42, 'name', 'msg').invalidValue"),
        Value::Int(42)
    );
    assert_eq!(
        expr("ArgumentError.value(42, 'name', 'msg').name"),
        Value::str("name")
    );
    assert_eq!(
        expr("ArgumentError.value(42, 'name', 'msg').message"),
        Value::str("msg")
    );
}

#[test]
fn format_exception() {
    assert_eq!(
        expr("FormatException('m').toString()"),
        Value::str("FormatException: m")
    );
    assert_eq!(
        expr("FormatException('m', 'source').toString()"),
        Value::str("FormatException: m")
    );
    assert_eq!(
        expr("FormatException('m', 's', 2).toString()"),
        Value::str("FormatException: m (at offset 2)")
    );
    assert_eq!(expr("FormatException('m', 's', 2).offset"), Value::Int(2));
    assert_eq!(expr("FormatException('m', 's', 2).source"), Value::str("s"));
    assert_eq!(
        expr("FormatException('m', 's', 2).message"),
        Value::str("m")
    );
}

#[test]
fn assert_statements() {
    assert_eq!(
        Context::new()
            .eval("void main() { assert(true); return 'ok'; }")
            .unwrap(),
        Value::str("ok")
    );
    assert_eq!(
        Context::new()
            .eval("void main() { assert(1 == 1); return 42; }")
            .unwrap(),
        Value::Int(42)
    );
}

#[test]
fn type_and_string_conversions() {
    assert_eq!(expr("(42).runtimeType.toString()"), Value::str("int"));
    assert_eq!(expr("('x').runtimeType.toString()"), Value::str("String"));
    assert_eq!(expr("(3.5).runtimeType.toString()"), Value::str("double"));
    assert_eq!(expr("(true).runtimeType.toString()"), Value::str("bool"));
    assert_eq!(expr("([1]).runtimeType.toString()"), Value::str("List"));
    assert_eq!(expr("({'a': 1}).runtimeType.toString()"), Value::str("Map"));
    assert_eq!(
        expr("(BigInt.from(1)).runtimeType.toString()"),
        Value::str("BigInt")
    );
    assert_eq!(
        expr("(Duration(seconds: 1)).runtimeType.toString()"),
        Value::str("Duration")
    );
    assert_eq!(
        expr("(DateTime(2024, 1, 1)).runtimeType.toString()"),
        Value::str("DateTime")
    );
    assert_eq!(
        expr("(RegExp('a')).runtimeType.toString()"),
        Value::str("RegExp")
    );
    assert_eq!(
        expr("(Uri.parse('https://x')).runtimeType.toString()"),
        Value::str("Uri")
    );
}

#[test]
fn stack_trace_and_misc() {
    assert_eq!(
        expr("StackTrace.current.toString().length > 0"),
        Value::Bool(true)
    );
    assert_eq!(expr("StackTrace.empty.toString()"), Value::str(""));
}
