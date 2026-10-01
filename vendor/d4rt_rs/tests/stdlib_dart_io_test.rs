use d4rt_rs::Context;
use d4rt_rs::permission::{FilesystemPermission, NetworkPermission, Permission, ProcessPermission};
use d4rt_rs::value::Value;

use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};

/// A `Context` with every capability granted (the pre-permission behavior).
fn permissive_ctx() -> Context {
    let mut c = Context::new();
    c.grant(Permission::Filesystem(FilesystemPermission::Any));
    c.grant(Permission::Network(NetworkPermission::Any));
    c.grant(Permission::Process(ProcessPermission::Any));
    c.grant(Permission::Isolate);
    c.grant(Permission::Env);
    c
}

static DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

fn fresh_dir(tag: &str) -> String {
    let n = DIR_COUNTER.fetch_add(1, Ordering::SeqCst);
    let d = std::env::temp_dir().join(format!("d4rt_rs_io_{tag}_{}_{}", std::process::id(), n));
    std::fs::create_dir_all(&d).unwrap();
    d.to_string_lossy().to_string()
}

fn ev(c: &mut Context, body: &str) -> Value {
    let src = format!("import 'dart:io';\nvoid main() {{ {body} }}");
    c.eval(&src)
        .unwrap_or_else(|e| panic!("eval failed: {:?}", e.message()))
}

fn r#async(c: &mut Context, body: &str, ret: &str) -> Value {
    let src = format!("import 'dart:io';\nFuture<dynamic> main() async {{ {body} return {ret}; }}");
    c.eval(&src)
        .unwrap_or_else(|e| panic!("eval failed: {:?} (debug: {e:?})", e.message()))
}

fn num(v: &Value) -> i64 {
    match v {
        Value::Int(i) => *i,
        other => panic!("expected int, got {other:?}"),
    }
}

fn boolean(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        other => panic!("expected bool, got {other:?}"),
    }
}

fn str(v: &Value) -> String {
    match v {
        Value::Str(s) => s.to_string(),
        other => panic!("expected string, got {other:?}"),
    }
}

#[test]
fn file_write_read_sync() {
    let mut c = permissive_ctx();
    let d = fresh_dir("wr");
    let v = ev(
        &mut c,
        &format!(
            "final f = File('{d}/a.txt');
             f.writeAsStringSync('hello');
             f.writeAsStringSync(' world', mode: FileMode.append);
             f.lengthSync();"
        ),
    );
    assert_eq!(num(&v), 11);
    let v = ev(
        &mut c,
        &format!("return File('{d}/a.txt').readAsStringSync();"),
    );
    assert_eq!(str(&v), "hello world");
}

#[test]
fn file_write_read_bytes() {
    let mut c = permissive_ctx();
    let d = fresh_dir("bytes");
    let v = ev(
        &mut c,
        &format!(
            "final f = File('{d}/b.bin');
             f.writeAsBytesSync([1, 2, 3]);
             f.readAsBytesSync().length;"
        ),
    );
    assert_eq!(num(&v), 3);
}

#[test]
fn file_exists_delete() {
    let mut c = permissive_ctx();
    let d = fresh_dir("exists");
    let v = ev(
        &mut c,
        &format!(
            "final f = File('{d}/c.txt');
             final before = f.existsSync();
             f.writeAsStringSync('x');
             final after = f.existsSync();
             f.deleteSync();
             after && !f.existsSync() && !before;"
        ),
    );
    assert!(boolean(&v));
    let v = ev(&mut c, &format!("return File('{d}/c.txt').existsSync();"));
    assert!(!boolean(&v));
}

#[test]
fn file_copy_rename() {
    let mut c = permissive_ctx();
    let d = fresh_dir("copy");
    let v = ev(
        &mut c,
        &format!(
            "final f = File('{d}/d.txt');
             f.writeAsStringSync('data');
             final g = f.copySync('{d}/d_copy.txt');
             final renamed = g.renameSync('{d}/d_moved.txt');
             renamed.readAsStringSync();"
        ),
    );
    assert_eq!(str(&v), "data");
    assert!(!std::path::Path::new(&format!("{d}/d_copy.txt")).exists());
    assert!(std::path::Path::new(&format!("{d}/d_moved.txt")).exists());
}

#[test]
fn file_stat_sync() {
    let mut c = permissive_ctx();
    let d = fresh_dir("stat");
    let v = ev(
        &mut c,
        &format!(
            "final f = File('{d}/e.txt');
             f.writeAsStringSync('hello');
             final s = f.statSync();
             s.size;"
        ),
    );
    assert_eq!(num(&v), 5);
    let v = ev(
        &mut c,
        &format!("return File('{d}/e.txt').statSync().type.name;"),
    );
    assert_eq!(str(&v), "file");
}

#[test]
fn file_async_apis() {
    let mut c = permissive_ctx();
    let d = fresh_dir("async");
    let v = r#async(
        &mut c,
        &format!(
            "final f = File('{d}/f.txt');
             await f.writeAsString('async data');
             final read = await f.readAsString();
             final size = (await f.stat()).size;
             final result = read.length + size;"
        ),
        "result",
    );
    assert_eq!(num(&v), 20);
    let v = r#async(
        &mut c,
        &format!("final f = File('{d}/f.txt'); final result = await f.readAsString();"),
        "result",
    );
    assert_eq!(str(&v), "async data");
}

#[test]
fn file_parent_absolute_uri() {
    let mut c = permissive_ctx();
    let d = fresh_dir("path");
    let v = ev(
        &mut c,
        &format!("final f = File('{d}/g.txt'); f.absolute.path;"),
    );
    assert!(str(&v).ends_with("g.txt"));
    let v = ev(&mut c, &format!("return File('{d}/g.txt').uri.toString();"));
    assert!(str(&v).starts_with("file://"));
    let v = ev(
        &mut c,
        &format!("final f = File('{d}/g.txt'); f.parent.path;"),
    );
    assert_eq!(str(&v), d);
}

#[test]
fn file_sync_error_throws() {
    let mut c = permissive_ctx();
    let v = ev(
        &mut c,
        "try { File('/definitely/missing/x.txt').readAsStringSync(); return 'no'; } catch (e) { return e.toString(); }",
    );
    assert!(str(&v).contains("FileSystemException"));
}

#[test]
fn file_async_error() {
    let mut c = permissive_ctx();
    let v = r#async(
        &mut c,
        "try { await File('/definitely/missing/x.txt').readAsString(); return 'no'; } catch (e) { return e.runtimeType.toString(); }",
        "null",
    );
    assert!(str(&v).contains("FileSystemException"));
}

#[test]
fn directory_create_list() {
    let mut c = permissive_ctx();
    let d = fresh_dir("list");
    let v = ev(
        &mut c,
        &format!(
            "final dir = Directory('{d}');
             final sub = Directory('{d}/sub'); sub.createSync();
             File('{d}/sub/a.txt').writeAsStringSync('a');
             File('{d}/sub/b.txt').writeAsStringSync('b');
             dir.listSync().length;"
        ),
    );
    assert_eq!(num(&v), 1);
    let v = ev(
        &mut c,
        &format!("return Directory('{d}/sub').listSync().length;"),
    );
    assert_eq!(num(&v), 2);
}

#[test]
fn directory_list_recursive() {
    let mut c = permissive_ctx();
    let d = fresh_dir("rec");
    let v = ev(
        &mut c,
        &format!(
            "final dir = Directory('{d}');
             final sub = Directory('{d}/sub2'); sub.createSync();
             File('{d}/sub2/a.txt').writeAsStringSync('a');
             File('{d}/sub2/b.txt').writeAsStringSync('b');
             dir.listSync(recursive: true).length;"
        ),
    );
    assert_eq!(num(&v), 3);
}

#[test]
fn directory_list_stream() {
    let mut c = permissive_ctx();
    let d = fresh_dir("stream");
    let v = r#async(
        &mut c,
        &format!(
            "final dir = Directory('{d}');
             final sub = Directory('{d}/sub3'); sub.createSync();
             File('{d}/sub3/a.txt').writeAsStringSync('a');
             File('{d}/sub3/b.txt').writeAsStringSync('b');
             final n = await dir.list().length;"
        ),
        "n",
    );
    assert_eq!(num(&v), 1);
}

#[test]
fn directory_create_temp() {
    let mut c = permissive_ctx();
    let v = ev(
        &mut c,
        "final t = Directory.createTempSync('d4rt_tmp'); final ok = t.existsSync(); t.deleteSync(recursive: true); ok;",
    );
    assert!(boolean(&v));
}

#[test]
fn file_system_entity_type() {
    let mut c = permissive_ctx();
    let d = fresh_dir("type");
    let v = ev(
        &mut c,
        &format!(
            "File('{d}/h.txt').writeAsStringSync('x');
             FileSystemEntity.typeSync('{d}/h.txt').name;"
        ),
    );
    assert_eq!(str(&v), "file");
    let v = ev(
        &mut c,
        &format!(
            "final sub = Directory('{d}/sub'); sub.createSync();
             FileSystemEntity.typeSync('{d}/sub').toString();"
        ),
    );
    assert_eq!(str(&v), "FileSystemEntityType(directory)");
    let v = ev(
        &mut c,
        &format!("return FileSystemEntity.isFileSync('{d}/h.txt');"),
    );
    assert!(boolean(&v));
    let v = ev(
        &mut c,
        &format!(
            "final sub = Directory('{d}/sub2'); sub.createSync();
             FileSystemEntity.isDirectorySync('{d}/sub2');"
        ),
    );
    assert!(boolean(&v));
}

#[test]
fn platform_getters() {
    let mut c = permissive_ctx();
    assert!(!str(&ev(&mut c, "return Platform.operatingSystem;")).is_empty());
    assert!(boolean(&ev(&mut c, "return Platform.isLinux;")));
    assert!(num(&ev(&mut c, "return Platform.numberOfProcessors;")) >= 1);
    assert_eq!(str(&ev(&mut c, "return Platform.pathSeparator;")), "/");
    assert!(boolean(&ev(
        &mut c,
        "return Platform.environment.containsKey('PATH');"
    )));
    assert!(str(&ev(&mut c, "return Platform.script.toString();")).starts_with("file://"));
}

#[test]
fn directory_current_system_temp() {
    let mut c = permissive_ctx();
    assert!(boolean(&ev(
        &mut c,
        "return Directory.current.existsSync();"
    )));
    assert!(boolean(&ev(
        &mut c,
        "return Directory.systemTemp.existsSync();"
    )));
}

#[test]
fn file_mode_overwrite_and_append() {
    let mut c = permissive_ctx();
    let d = fresh_dir("mode");
    let v = ev(
        &mut c,
        &format!(
            "final f = File('{d}/m.txt');
             f.writeAsStringSync('one');
             f.writeAsStringSync('two');
             f.readAsStringSync();"
        ),
    );
    assert_eq!(str(&v), "two");
    let v = ev(
        &mut c,
        &format!(
            "final f = File('{d}/m.txt');
             f.writeAsStringSync('!', mode: FileMode.append);
             f.readAsStringSync();"
        ),
    );
    assert_eq!(str(&v), "two!");
}

#[test]
fn random_access_file_write_read() {
    let mut c = permissive_ctx();
    let d = fresh_dir("raf");
    let v = r#async(
        &mut c,
        &format!(
            "final raf = await File('{d}/f.bin').open(mode: FileMode.write);
             await raf.writeString('Hello');
             await raf.writeByte(33);
             await raf.writeFrom([4, 5, 6]);
             final n = await raf.length();
             await raf.close();
             n;"
        ),
        "n",
    );
    assert_eq!(num(&v), 9);
    let v = r#async(
        &mut c,
        &format!(
            "final raf = await File('{d}/f.bin').open();
             final b = <int>[0, 0, 0];
             final n = await raf.readInto(b);
             final rest = await raf.read(6);
             final eof = await raf.readByte();
             await raf.close();
             [n, b[0], b[1], b[2], String.fromCharCodes(rest), eof == null];"
        ),
        "[n, b[0], b[1], b[2], String.fromCharCodes(rest), eof == null]",
    );
    let vals = match v {
        Value::List(l) => l.borrow().clone(),
        other => panic!("expected list, got {other:?}"),
    };
    assert_eq!(num(&vals[0]), 3);
    assert_eq!(num(&vals[1]), 72);
    assert_eq!(num(&vals[2]), 101);
    assert_eq!(num(&vals[3]), 108);
    assert_eq!(str(&vals[4]), "lo!\u{4}\u{5}\u{6}");
    assert!(boolean(&vals[5]));
}

#[test]
fn random_access_file_position_truncate_sync() {
    let mut c = permissive_ctx();
    let d = fresh_dir("rafpos");
    let v = ev(
        &mut c,
        &format!(
            "final f = File('{d}/t.bin');
             f.writeAsStringSync('abcdefgh');
             final raf = f.openSync();
             String.fromCharCodes(raf.readSync(4));"
        ),
    );
    assert_eq!(str(&v), "abcd");
    let v = ev(
        &mut c,
        &format!(
            "final raf = File('{d}/t.bin').openSync(mode: FileMode.append);
             raf.positionSync();"
        ),
    );
    assert_eq!(num(&v), 8);
    let v = ev(
        &mut c,
        &format!(
            "final raf = File('{d}/t.bin').openSync(mode: FileMode.write);
             raf.writeStringSync('XY');
             raf.lengthSync();"
        ),
    );
    assert_eq!(num(&v), 2);
    let v = ev(
        &mut c,
        &format!(
            "final f = File('{d}/t.bin');
             f.writeAsStringSync('abcdef');
             final raf = f.openSync();
             raf.truncateSync(3);
             raf.closeSync();
             f.readAsStringSync();"
        ),
    );
    assert_eq!(str(&v), "abc");
}

#[test]
fn random_access_file_append_and_rewrite() {
    let mut c = permissive_ctx();
    let d = fresh_dir("rafappend");
    let v = r#async(
        &mut c,
        &format!(
            "final raf = await File('{d}/a.txt').open(mode: FileMode.append);
             await raf.writeString('one');
             await raf.close();
             final raf2 = await File('{d}/a.txt').open(mode: FileMode.append);
             await raf2.writeString('two');
             await raf2.close();
             File('{d}/a.txt').readAsStringSync();"
        ),
        &format!("File('{d}/a.txt').readAsStringSync();"),
    );
    assert_eq!(str(&v), "onetwo");
    let v = ev(
        &mut c,
        &format!(
            "final raf = File('{d}/a.txt').openSync(mode: FileMode.write);
             raf.writeStringSync('new');
             raf.closeSync();
             File('{d}/a.txt').readAsStringSync();"
        ),
    );
    assert_eq!(str(&v), "new");
}

#[test]
fn process_run_and_run_sync() {
    let mut c = permissive_ctx();
    let v = r#async(
        &mut c,
        "final r = await Process.run('echo', ['hello', 'world']);
         [r.exitCode, r.pid > 0, r.stdout.trim(), r.stderr.toString()];",
        "[r.exitCode, r.pid > 0, r.stdout.trim(), r.stderr.toString()]",
    );
    let vals = match v {
        Value::List(l) => l.borrow().clone(),
        other => panic!("expected list, got {other:?}"),
    };
    assert_eq!(num(&vals[0]), 0);
    assert!(boolean(&vals[1]));
    assert_eq!(str(&vals[2]), "hello world");
    assert_eq!(str(&vals[3]), "");
    let v = ev(
        &mut c,
        "final r = Process.runSync('sh', ['-c', 'echo out; echo err >&2; exit 3']);
         [r.exitCode, r.stdout.trim(), r.stderr.trim()];",
    );
    let vals = match v {
        Value::List(l) => l.borrow().clone(),
        other => panic!("expected list, got {other:?}"),
    };
    assert_eq!(num(&vals[0]), 3);
    assert_eq!(str(&vals[1]), "out");
    assert_eq!(str(&vals[2]), "err");
}

#[test]
fn process_start_streams_and_exit_code() {
    let mut c = permissive_ctx();
    let v = r#async(
        &mut c,
        "final p = await Process.start('sh', ['-c', 'echo one; echo two; exit 3']);
         final out = await p.stdout.transform(utf8.decoder).join('\n');
         final code = await p.exitCode;
         [code, out.trim().split('\n').where((l) => l.isNotEmpty).join(',')];",
        "[code, out.trim().split('\n').where((l) => l.isNotEmpty).join(',')]",
    );
    let vals = match v {
        Value::List(l) => l.borrow().clone(),
        other => panic!("expected list, got {other:?}"),
    };
    assert_eq!(num(&vals[0]), 3);
    assert_eq!(str(&vals[1]), "one,two");
}

#[test]
fn process_start_stdin_pipe() {
    let mut c = permissive_ctx();
    let v = r#async(
        &mut c,
        "final p = await Process.start('cat');
         p.stdin.writeln('x1');
         p.stdin.writeln('x2');
         await p.stdin.close();
         final lines = await p.stdout.transform(utf8.decoder).join('\\n');
         final code = await p.exitCode;
         lines.contains('x1') && lines.contains('x2');",
        "lines.contains('x1') && lines.contains('x2')",
    );
    assert!(boolean(&v));
}

#[test]
fn process_kill() {
    let mut c = permissive_ctx();
    let v = r#async(
        &mut c,
        "final p = await Process.start('sh', ['-c', 'sleep 30']);
         final killed = p.kill();
         final code = await p.exitCode;
         killed && code != 0;",
        "killed && code != 0",
    );
    assert!(boolean(&v));
}

#[test]
fn process_run_options_and_exception() {
    let mut c = permissive_ctx();
    let v = r#async(
        &mut c,
        "final r = await Process.run('pwd', workingDirectory: '/tmp');
         r.stdout.trim();",
        "r.stdout.trim()",
    );
    assert_eq!(str(&v), "/tmp");
    let v = r#async(
        &mut c,
        "final r = await Process.run('echo', ['hello'], runInShell: true);
         r.stdout.trim();",
        "r.stdout.trim()",
    );
    assert_eq!(str(&v), "hello");
    let v = r#async(
        &mut c,
        "try {
           await Process.run('/definitely/not/a/binary');
           return 'no';
         } catch (e) {
           return e.runtimeType.toString();
         }",
        "'x'",
    );
    assert_eq!(str(&v), "ProcessException");
}

#[test]
fn stdio_stdout_write_methods() {
    let mut c = permissive_ctx();
    let v = ev(
        &mut c,
        "stdout.write('x'); stdout.writeln(); stdout.writeln('y');
         stdout.writeAll([1, 'two', 3], '-');
         stdout.add([72, 105]);
         stdout.addError('ignored');
         [stdout.write(''), stdout.writeln('z'), stdout.toString()];",
    );
    let vals = match v {
        Value::List(l) => l.borrow().clone(),
        other => panic!("expected list, got {other:?}"),
    };
    assert!(matches!(vals[0], Value::Null));
    assert!(matches!(vals[1], Value::Null));
    assert_eq!(str(&vals[2]), "Stdout");
}

#[test]
fn stdio_stdout_encoding_and_close() {
    let mut c = permissive_ctx();
    let v = r#async(
        &mut c,
        "final enc = stdout.encoding;
         stdout.encoding = enc;
         final a = await stdout.flush();
         final b = await stdout.close();
         final d = await stdout.done;
         ['${enc.runtimeType}', a == null, b == null, d == null];",
        "['${enc.runtimeType}', a == null, b == null, d == null]",
    );
    let vals = match v {
        Value::List(l) => l.borrow().clone(),
        other => panic!("expected list, got {other:?}"),
    };
    assert_eq!(str(&vals[0]), "Utf8Codec");
    for v in vals.iter().take(4).skip(1) {
        assert!(boolean(v));
    }
}

#[test]
fn stdio_stderr_and_stdin_eof() {
    let mut c = permissive_ctx();
    let v = ev(
        &mut c,
        "stderr.writeln('e');
         final l = stdin.hasTerminal ? 'skip' : stdin.readLineSync();
         final b = stdin.hasTerminal ? -1 : (stdin.readByteSync() ?? -2);
         [stderr.toString(), l ?? 'null', b];",
    );
    let vals = match v {
        Value::List(l) => l.borrow().clone(),
        other => panic!("expected list, got {other:?}"),
    };
    assert_eq!(str(&vals[0]), "Stderr");
    // `stdin.hasTerminal` mirrors the host's stdin: interactive terminals
    // take the "skip" probe branch, redirected stdin (CI, pipes) observes
    // EOF and yields null / -2.
    assert!(str(&vals[1]) == "skip" || str(&vals[1]) == "null");
    assert!(num(&vals[2]) == -1 || num(&vals[2]) == -2);
}

#[test]
fn stdio_type_statics() {
    let mut c = permissive_ctx();
    let v = ev(
        &mut c,
        "['${StdioType.terminal}', '${StdioType.pipe}', '${StdioType.file}', '${StdioType.other}'].join(',');",
    );
    assert_eq!(
        str(&v),
        "StdioType.terminal,StdioType.pipe,StdioType.file,StdioType.other"
    );
}

#[test]
fn stdio_stdout_add_stream() {
    let mut c = permissive_ctx();
    let v = r#async(
        &mut c,
        "final sc = StreamController<List<int>>();
         sc.add([72, 105]);
         sc.add('!'.codeUnits);
         await sc.close();
         final r = await stdout.addStream(sc.stream);
         r == null;",
        "r == null",
    );
    assert!(boolean(&v));
}

fn http_server_thread(response: String) -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut buf = Vec::new();
            let mut tmp = [0u8; 4096];
            loop {
                match stream.read(&mut tmp) {
                    Ok(0) => break,
                    Ok(n) => buf.extend_from_slice(&tmp[..n]),
                    Err(_) => break,
                }
                if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            let _ = std::io::Write::write_all(&mut stream, response.as_bytes());
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
    });
    port
}

fn echo_server_thread() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut buf = [0u8; 256];
            if let Ok(n) = stream.read(&mut buf) {
                let _ = std::io::Write::write_all(&mut stream, &buf[..n]);
            }
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
    });
    port
}

#[test]
fn socket_connect_echo() {
    let mut c = permissive_ctx();
    let port = echo_server_thread();
    let v = r#async(
        &mut c,
        &format!(
            "final s = await Socket.connect('127.0.0.1', {port});
             s.write('hello');
             final first = await s.first;
             s.close();
             utf8.decode(first);"
        ),
        "utf8.decode(first)",
    );
    assert_eq!(str(&v), "hello");
}

#[test]
fn server_socket_accept_and_echo() {
    let mut c = permissive_ctx();
    let v = r#async(
        &mut c,
        "final ss = await ServerSocket.bind(InternetAddress.anyIPv4, 0);
         final port = ss.port;
         final sub = ss.listen((s) {
           s.write('welcome');
           s.close();
         });
         final tcp = await Socket.connect('127.0.0.1', port);
         final first = await tcp.first;
         tcp.close();
         sub.cancel();
         await ss.close();
         utf8.decode(first);",
        "utf8.decode(first)",
    );
    assert_eq!(str(&v), "welcome");
}

#[test]
fn http_client_get() {
    let mut c = permissive_ctx();
    let port =
        http_server_thread("HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\nhello world".to_string());
    let v = r#async(
        &mut c,
        &format!(
            "final client = HttpClient();
             final req = await client.getUrl(Uri.parse('http://127.0.0.1:{port}/foo'));
             final resp = await req.close();
             final status = resp.statusCode;
             final reason = resp.reasonPhrase;
             final text = await resp.transform(utf8.decoder).join();
             final len = resp.contentLength;
             '{{status}}:{{reason}}:{{text}}:{{len}}';"
        ),
        "'${status}:${reason}:${text}:${len}'",
    );
    assert_eq!(str(&v), "200:OK:hello world:11");
}

#[test]
fn http_server_roundtrip() {
    let mut c = permissive_ctx();
    let v = r#async(
        &mut c,
        "final server = await HttpServer.bind(InternetAddress.anyIPv4, 0);
         final port = server.port;
         server.listen((req) {
           req.transform(utf8.decoder).join().then((body) {
             req.response.statusCode = 201;
             req.response.write('echo:$body');
             req.response.close();
           });
         });
         final client = HttpClient();
         final req = await client.postUrl(Uri.parse('http://127.0.0.1:${port}/hello'));
         req.write('ping');
         final resp = await req.close();
         final st = resp.statusCode;
         final text = await resp.transform(utf8.decoder).join();
         await server.close();
         '${st}:${text}';",
        "'${st}:${text}'",
    );
    assert_eq!(str(&v), "201:echo:ping");
}

fn r#async_isolate(c: &mut Context, body: &str, ret: &str) -> Value {
    let src = format!(
        "import 'dart:isolate';\nimport 'dart:typed_data';\nFuture<dynamic> main() async {{ {body} return {ret}; }}"
    );
    c.eval(&src)
        .unwrap_or_else(|e| panic!("eval failed: {:?} (debug: {e:?})", e.message()))
}

#[test]
fn isolate_send_receive() {
    let mut c = permissive_ctx();
    let v = r#async_isolate(
        &mut c,
        "final rp = ReceivePort();
         rp.sendPort.send('ping');
         rp.sendPort.send('pong');
         final msgs = await rp.take(2).toList();
         rp.close();
         msgs.join(',');",
        "msgs.join(',');",
    );
    assert_eq!(str(&v), "ping,pong");
}

#[test]
fn isolate_receive_buffered_before_listen() {
    let mut c = permissive_ctx();
    let v = r#async_isolate(
        &mut c,
        "final rp = ReceivePort();
         rp.sendPort.send(41);
         final m = await rp.first;
         rp.close();
         m + 1;",
        "m + 1",
    );
    assert_eq!(num(&v), 42);
}

#[test]
fn isolate_run_sync_and_async() {
    let mut c = permissive_ctx();
    let v = r#async_isolate(
        &mut c,
        "final a = await Isolate.run(() => 21 * 2);
         final b = await Isolate.run(() async => 30 + 12);
         a + b;",
        "a + b",
    );
    assert_eq!(num(&v), 84);
}

#[test]
fn isolate_run_error_propagates() {
    let mut c = permissive_ctx();
    let v = r#async_isolate(
        &mut c,
        "var msg = '';
         try {
           await Isolate.run(() { throw 'boom'; });
           msg = 'no-error';
         } catch (e) {
           msg = 'caught:${e}';
         }
         msg;",
        "msg",
    );
    assert_eq!(str(&v), "caught:boom");
}

#[test]
fn isolate_raw_receive_port_handler() {
    let mut c = permissive_ctx();
    let v = r#async_isolate(
        &mut c,
        "final seen = <int>[];
         final rp = RawReceivePort((m) { seen.add(m as int); });
         rp.sendPort.send(5);
         rp.sendPort.send(7);
         rp.close();
         seen.join(',');",
        "seen.join(',');",
    );
    assert_eq!(str(&v), "5,7");
}

#[test]
fn isolate_spawn_returns_isolate() {
    let mut c = permissive_ctx();
    let v = r#async_isolate(
        &mut c,
        "final iso = await Isolate.spawn((msg) { msg as int; }, 5);
         final ok = iso.controlPort != null && Isolate.beforeNextEvent == 0 && Isolate.immediate == 1;
         ok;",
        "ok",
    );
    assert!(boolean(&v));
}

#[test]
fn isolate_exceptions_and_remote_error() {
    let mut c = permissive_ctx();
    let v = r#async_isolate(
        &mut c,
        "final e = IsolateSpawnException('spawn failed');
         final r = RemoteError('desc', 'stack');
         '${e.message}|${e}|${r.stackTrace}|${r}';",
        "'${e.message}|${e}|${r.stackTrace}|${r}'",
    );
    assert_eq!(
        str(&v),
        "spawn failed|IsolateSpawnException: spawn failed|stack|RemoteError: desc"
    );
}

#[test]
fn isolate_exit_sends_message() {
    let mut c = permissive_ctx();
    let v = r#async_isolate(
        &mut c,
        "final rp = ReceivePort();
         Isolate.exit(rp.sendPort, 'bye');
         final m = await rp.first;
         rp.close();
         m;",
        "m",
    );
    assert_eq!(str(&v), "bye");
}

#[test]
fn isolate_transferable_typed_data() {
    let mut c = permissive_ctx();
    let v = r#async_isolate(
        &mut c,
        "final t = TransferableTypedData.fromList([Uint8List.fromList([1, 2, 3])]);
         final m = t.materialize();
         '${m.length}:${m[0]},${m[1]},${m[2]}';",
        "'${m.length}:${m[0]},${m[1]},${m[2]}'",
    );
    assert_eq!(str(&v), "3:1,2,3");
}

// ---------- dart:io completeness (Batch A) ----------

#[test]
fn file_from_uri_is_absolute_parent() {
    let mut c = permissive_ctx();
    let d = fresh_dir("furi");
    let v = ev(
        &mut c,
        &format!(
            "final f = File.fromUri(Uri.parse('file://{d}/x.txt'));
             [f.isAbsolute, f.parent.isAbsolute, f.uri.toString().startsWith('file://')]
             .join(',');"
        ),
    );
    assert_eq!(str(&v), "true,true,true");
}

#[test]
fn file_identical_static_and_instance() {
    let mut c = permissive_ctx();
    let d = fresh_dir("fid");
    let p = format!("{d}/y.txt");
    std::fs::write(&p, "a").unwrap();
    let v = ev(
        &mut c,
        &format!(
            "final same = FileSystemEntity.identicalSync('{p}', '{p}');
             final diff = File('{p}').identicalSync('{d}/z.txt');
             [same, diff].join(',');"
        ),
    );
    assert_eq!(str(&v), "true,false");
}

#[test]
fn file_read_as_lines() {
    let mut c = permissive_ctx();
    let d = fresh_dir("flines");
    let v = ev(
        &mut c,
        &format!(
            "final f = File('{d}/l.txt');
             f.writeAsStringSync('a\\nb\\n\\nc');
             f.readAsLinesSync().join('|');"
        ),
    );
    assert_eq!(str(&v), "a|b||c");
    let v = r#async(
        &mut c,
        &format!("final l = await File('{d}/l.txt').readAsLines();"),
        "l.length",
    );
    assert_eq!(num(&v), 4);
}

#[cfg(unix)]
#[test]
fn file_resolve_symbolic_links() {
    let mut c = permissive_ctx();
    let d = fresh_dir("fsym");
    let real = format!("{d}/real.txt");
    std::fs::write(&real, "x").unwrap();
    std::os::unix::fs::symlink(&real, format!("{d}/link.txt")).unwrap();
    let v = ev(
        &mut c,
        &format!("File('{d}/link.txt').resolveSymbolicLinksSync();"),
    );
    assert_eq!(str(&v), real);
    let v = ev(
        &mut c,
        &format!("Directory('{d}/link.txt').parent.resolveSymbolicLinksSync();"),
    );
    assert_eq!(str(&v), d);
}

#[test]
fn file_times_stat_mode_string() {
    let mut c = permissive_ctx();
    let d = fresh_dir("ftimes");
    let v = ev(
        &mut c,
        &format!(
            "final f = File('{d}/t.txt');
             f.writeAsStringSync('hello');
             f.setLastModifiedSync(DateTime(2020, 1, 2, 3, 4, 5));
             final st = f.statSync();
             final t = f.lastModifiedSync();
             [st.size, t.year, st.modeString.startsWith('-'), st.type.name, st.type == FileSystemEntityType.file]
             .join(',');"
        ),
    );
    assert_eq!(str(&v), "5,2020,true,file,true");
}

#[test]
fn file_create_temp_instance() {
    let mut c = permissive_ctx();
    let d = fresh_dir("ftemp");
    let v = ev(
        &mut c,
        &format!(
            "final f = File('{d}/base.txt').createTempSync(prefix: 'pfx', suffix: '.tmp');
             final s = f.existsSync();
             f.deleteSync();
             [s, f.path.contains('pfx'), f.path.endsWith('.tmp')].join(',');"
        ),
    );
    assert_eq!(str(&v), "true,true,true");
}

#[test]
fn file_open_write_sink() {
    let mut c = permissive_ctx();
    let d = fresh_dir("fows");
    let v = r#async(
        &mut c,
        &format!(
            "final f = File('{d}/sink.txt');
             final sink = f.openWrite();
             sink.write('hello ');
             sink.writeln('world');
             sink.writeAll(['a', 'b'], '|');
             sink.writeCharCode(33);
             await sink.close();"
        ),
        "f.readAsStringSync()",
    );
    assert_eq!(str(&v), "hello world\na|b!");
}

#[test]
fn file_open_read_stream() {
    let mut c = permissive_ctx();
    let d = fresh_dir("fors");
    let v = r#async(
        &mut c,
        &format!(
            "final f = File('{d}/s.txt');
             f.writeAsStringSync('stream-data');
             final chunks = await f.openRead().toList();"
        ),
        "chunks.map((b) => String.fromCharCodes(b)).join()",
    );
    assert_eq!(str(&v), "stream-data");
}

#[test]
fn path_exceptions_and_file_lock() {
    let mut c = permissive_ctx();
    let v = ev(
        &mut c,
        "final e = PathNotFoundException('/nope/x');
         final a = PathAccessException('/nope/x');
         final x = PathExistsException('/nope/x');
         [e.message, a.path, x.message, FileLock.shared.name, FileLock.blockingExclusive.name, FileSystemEvent.create, FileSystemEvent.move]
         .join(',');",
    );
    assert_eq!(
        str(&v),
        "Cannot open file,/nope/x,File exists,shared,blockingExclusive,1,8"
    );
}

#[test]
fn directory_from_uri_and_temp_instance() {
    let mut c = permissive_ctx();
    let d = fresh_dir("duri");
    let v = ev(
        &mut c,
        &format!(
            "final dir = Directory.fromUri(Uri.parse('file://{d}'));
             final t = dir.createTempSync('sub_');
             final s = t.existsSync();
             t.deleteSync();
             [dir.isAbsolute, dir.uri.toString().startsWith('file://'), s, t.path.contains('sub_')]
             .join(',');"
        ),
    );
    assert_eq!(str(&v), "true,true,true,true");
}

#[test]
fn platform_package_config() {
    let mut c = permissive_ctx();
    let v = ev(
        &mut c,
        "Platform.packageConfig == null || Platform.packageConfig.toString().startsWith('file:');",
    );
    assert!(boolean(&v));
}

#[test]
fn fs_event_constants_and_watch_create() {
    let mut c = permissive_ctx();
    let d = fresh_dir("watch");
    let created = format!("{d}/new.txt");
    let created_t = created.clone();
    let t = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));
        std::fs::write(&created_t, "x").unwrap();
    });
    let v = r#async(
        &mut c,
        &format!(
            "final ev = await Directory('{d}')
                .watch(FileSystemEvent.create)
                .first;"
        ),
        "ev.isCreate ? ev.path : 'WRONG:' + ev.path",
    );
    t.join().unwrap();
    assert_eq!(str(&v), created);
}

#[test]
fn watch_file_modify() {
    let mut c = permissive_ctx();
    let d = fresh_dir("watchm");
    let f = format!("{d}/f.txt");
    std::fs::write(&f, "v1").unwrap();
    let f_t = f.clone();
    let t = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));
        std::fs::write(&f_t, "v2-longer").unwrap();
    });
    let v = r#async(
        &mut c,
        &format!(
            "final ev = await File('{f}')
                .watch(FileSystemEvent.modify)
                .first;"
        ),
        "ev.isModify",
    );
    t.join().unwrap();
    assert!(boolean(&v));
}

#[test]
fn socket_option_direction_raw_event_enums() {
    let mut c = permissive_ctx();
    let v = ev(
        &mut c,
        "[SocketOption.tcpNoDelay.name, SocketDirection.receive.name, SocketDirection.send.name, SocketDirection.both.name, RawSocketEvent.read.name, RawSocketEvent.write.name, RawSocketEvent.readClosed.name, RawSocketEvent.closed.name].join(',');",
    );
    assert_eq!(
        str(&v),
        "tcpNoDelay,receive,send,both,read,write,readClosed,closed"
    );
}

#[test]
fn raw_socket_option_ctors_and_levels() {
    let mut c = permissive_ctx();
    let v = ev(
        &mut c,
        "final a = RawSocketOption(1, 2, [1, 2, 3]);
         final b = RawSocketOption.fromInt(6, 7, 0x01020304);
         final d = RawSocketOption.fromBool(17, 8, true);
         [a.level, a.option, a.value.length, b.level, b.option, b.value.length, d.value[0], RawSocketOption.levelSocket, RawSocketOption.levelTcp, RawSocketOption.levelUdp].join(',');",
    );
    assert_eq!(str(&v), "1,2,3,6,7,4,1,1,6,17");
}

#[test]
fn socket_start_connect_connection_task() {
    let mut c = permissive_ctx();
    let port = echo_server_thread();
    let v = r#async(
        &mut c,
        &format!(
            "final task = Socket.startConnect('127.0.0.1', {port});
             final s = await task.socket;
             s.write('hi');
             final first = await s.first;
             s.close();
             utf8.decode(first);"
        ),
        "utf8.decode(first)",
    );
    assert_eq!(str(&v), "hi");
}

#[test]
fn raw_socket_connect_echo_read() {
    let mut c = permissive_ctx();
    let port = echo_server_thread();
    let v = r#async(
        &mut c,
        &format!(
            "final s = await RawSocket.connect('127.0.0.1', {port});
             s.write([104, 101, 108, 108, 111]);
             final ev = await s.first;
             final data = await s.read(64);
             s.close();
             utf8.decode(data);"
        ),
        "utf8.decode(data)",
    );
    assert_eq!(str(&v), "hello");
}

#[test]
fn raw_server_socket_bind_accept() {
    let mut c = permissive_ctx();
    let v = r#async(
        &mut c,
        "final ss = await RawServerSocket.bind(InternetAddress.anyIPv4, 0);
         final port = ss.port;
         final sub = ss.listen((s) {
           s.write([119, 101, 108, 99, 111, 109, 101]);
           s.close();
         });
         final tcp = await RawSocket.connect('127.0.0.1', port);
         tcp.write([120]);
         final ev = await tcp.first;
         final data = await tcp.read(64);
         tcp.close();
         sub.cancel();
         await ss.close();
         data == null ? 'none' : utf8.decode(data);",
        "data == null ? 'none' : utf8.decode(data)",
    );
    assert_eq!(str(&v), "welcome");
}

#[test]
fn raw_datagram_socket_udp_roundtrip() {
    let mut c = permissive_ctx();
    let v = r#async(
        &mut c,
        "final a = await RawDatagramSocket.bind('127.0.0.1', 0);
         final b = await RawDatagramSocket.bind('127.0.0.1', 0);
         a.send([104, 105], InternetAddress.loopbackIPv4, b.port);
         final ev = await b.first;
         final d = b.receive();
         a.close();
         b.close();
         [d.data.join(','), d.address.host, d.port == a.port, b.port > 0, a.broadcastEnabled].join('|');",
        "[d.data.join(','), d.address.host, d.port == a.port, b.port > 0, a.broadcastEnabled].join('|')",
    );
    assert_eq!(str(&v), "104,105|127.0.0.1|true|true|false");
}

#[test]
fn network_interface_list() {
    let mut c = permissive_ctx();
    let v = r#async(
        &mut c,
        "final list = await NetworkInterface.list();
         list.isNotEmpty && list[0].name.isNotEmpty && list[0].addresses.isNotEmpty && list[0].index > 0;",
        "list.isNotEmpty && list[0].name.isNotEmpty && list[0].addresses.isNotEmpty && list[0].index > 0",
    );
    assert!(boolean(&v));
}

#[test]
fn pipe_create_sync_roundtrip() {
    let mut c = permissive_ctx();
    let v = r#async(
        &mut c,
        "final p = Pipe.createSync();
         final all = p.read.toList();
         p.write.write('hello ');
         p.write.writeln('world');
         await p.write.close();
         final parts = await all;
         utf8.decode(parts.expand((x) => x).toList());",
        "utf8.decode(parts.expand((x) => x).toList())",
    );
    assert_eq!(str(&v), "hello world\n");
}

#[test]
fn socket_set_option_and_add_stream() {
    let mut c = permissive_ctx();
    let port = echo_server_thread();
    let v = r#async(
        &mut c,
        &format!(
            "final s = await Socket.connect('127.0.0.1', {port});
             s.setOption(SocketOption.tcpNoDelay, true);
             final src = Stream.fromIterable([utf8.encode('abcdef')]);
             await s.addStream(src);
             final first = await s.first;
             s.close();
             utf8.decode(first);"
        ),
        "utf8.decode(first)",
    );
    assert_eq!(str(&v), "abcdef");
}

#[test]
fn header_value_parse_and_params() {
    let mut c = permissive_ctx();
    let v = r#async(
        &mut c,
        "final h = HeaderValue.parse('text/html; charset=utf-8');
         final q = HeaderValue.parse('a; name=\"x\\\\y\"');
         final p = HeaderValue.parse('v; k=1; flag; quoted=\"2; 3\"', parameterSeparator: ';');
         final vs = HeaderValue.parse('key=value; p=1', valueSeparator: '=');
         [h.value, h.parameters['charset'], h.toString(), q.parameters['name'], p.parameters['flag'] == null ? 'nul' : 'set', p.parameters['quoted'], vs.value].join('|');",
        "[h.value, h.parameters['charset'], h.toString(), q.parameters['name'], p.parameters['flag'] == null ? 'nul' : 'set', p.parameters['quoted'], vs.value].join('|')",
    );
    assert_eq!(
        str(&v),
        "text/html|utf-8|text/html; charset=utf-8|xy|nul|2; 3|key"
    );
}

#[test]
fn header_value_ctor_and_preserve_backslash() {
    let mut c = permissive_ctx();
    let v = r#async(
        &mut c,
        "final h = HeaderValue('x', {'a': 'b', 'c': null});
         final pb = HeaderValue.parse('a; name=\"x\\\\y\"', preserveBackslash: true);
         [h.toString(), h.value, h.parameters['a'], h.parameters.containsKey('c'), pb.parameters['name']].join('|');",
        "[h.toString(), h.value, h.parameters['a'], h.parameters.containsKey('c'), pb.parameters['name']].join('|')",
    );
    assert_eq!(str(&v), "x; a=b;c|x|b|true|x\\y");
}

#[test]
fn internet_address_from_raw_address() {
    let mut c = permissive_ctx();
    let v = r#async(
        &mut c,
        "final a = InternetAddress.fromRawAddress([127, 0, 0, 1]);
         final b = InternetAddress.fromRawAddress([0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0], type: InternetAddressType.IPv6);
         '${a.address}|${a.type.name}|${b.address}';",
        "'${a.address}|${a.type.name}|${b.address}'",
    );
    assert_eq!(str(&v), "127.0.0.1|IPv4|2001:db8:0:0");
}

#[test]
fn socket_exception_closed_ctor() {
    let mut c = permissive_ctx();
    let v = r#async(
        &mut c,
        "final e = SocketException.closed('bye', 'host', 99);
         e.toString();",
        "e.toString()",
    );
    assert_eq!(str(&v), "SocketException: bye, host = host, port = 99");
}

#[test]
fn file_system_entity_parent_of() {
    let mut c = permissive_ctx();
    let v = r#async(
        &mut c,
        "final p = FileSystemEntity.parentOf('/a/b/c.txt');
         p.path;",
        "p.path",
    );
    assert_eq!(str(&v), "/a/b");
}

#[test]
fn server_socket_stream_methods() {
    let mut c = permissive_ctx();
    let v = r#async(
        &mut c,
        "final server = await ServerSocket.bind('127.0.0.1', 0);
         final port = server.port;
         final client = await Socket.connect('127.0.0.1', port);
         final sockets = await server.take(1).toList();
         client.destroy();
         await sockets[0].close();
         await server.close();
         sockets.length;",
        "sockets.length",
    );
    assert_eq!(num(&v), 1);
}

#[test]
fn iosink_add_stream() {
    let d = fresh_dir("sink");
    let mut c = permissive_ctx();
    let v = r#async(
        &mut c,
        &format!(
            "final f = File('{d}/out.bin');
             final sink = f.openWrite();
             await sink.addStream(Stream.fromIterable([
                 [1, 2, 3],
                 [4, 5],
             ]));
             await sink.flush();
             await sink.close();
             final bytes = await f.readAsBytes();
             bytes.join(',');",
        ),
        "bytes.join(',')",
    );
    assert_eq!(str(&v), "1,2,3,4,5");
}

#[test]
fn isolate_receive_port_from_raw_receive_port() {
    let mut c = permissive_ctx();
    let v = r#async_isolate(
        &mut c,
        "final raw = RawReceivePort();
         final rp = ReceivePort.fromRawReceivePort(raw);
         rp.sendPort.send('hello');
         final m = await rp.first;
         rp.close();
         m;",
        "m",
    );
    assert_eq!(str(&v), "hello");
}

#[test]
fn isolate_debug_name_and_pause_resume() {
    let mut c = permissive_ctx();
    let v = r#async_isolate(
        &mut c,
        "final iso = Isolate(debugName: 'worker');
         final cap = iso.pauseCapability;
         final term = iso.terminateCapability;
         final f = iso.pause();
         iso.resume('r');
         final r = await f;
         '${iso.debugName}|${r}|${cap == null}|${term == null}';",
        "'${iso.debugName}|${r}|${cap == null}|${term == null}'",
    );
    assert_eq!(str(&v), "worker|r|true|true");
}

#[test]
fn isolate_capabilities_and_exit_listener() {
    let mut c = permissive_ctx();
    let v = r#async_isolate(
        &mut c,
        "final pc = Capability();
         final tc = Capability();
         final iso = Isolate(pauseCapability: pc, terminateCapability: tc);
         final rp = ReceivePort();
         iso.addOnExitListener(rp.sendPort, response: 'bye');
         iso.kill();
         final m = await rp.first;
         rp.close();
         '${iso.pauseCapability == pc}|${iso.terminateCapability == tc}|${m}';",
        "'${iso.pauseCapability == pc}|${iso.terminateCapability == tc}|${m}'",
    );
    assert_eq!(str(&v), "true|true|bye");
}

#[test]
fn raf_closed_and_lock_behavior() {
    let d = fresh_dir("raflock");
    let mut c = permissive_ctx();
    let v = r#async(
        &mut c,
        &format!(
            "final f = File('{d}/raf.bin');
             f.writeAsStringSync('hello');
             final raf = f.openSync();
             await raf.lock();
             String r = '';
             try {{ await raf.lock(); r += 'no|'; }} catch (e) {{ r += 'locked|'; }}
             await raf.unlock();
             await raf.lock();
             await raf.unlock();
             raf.closeSync();
             try {{ raf.readByteSync(); r += 'no'; }} catch (e) {{ r += 'closed'; }}
             r;",
        ),
        "r",
    );
    assert_eq!(str(&v), "locked|closed");
}

#[test]
fn raf_flush_sync_after_write() {
    let d = fresh_dir("rafflush");
    let mut c = permissive_ctx();
    let v = r#async(
        &mut c,
        &format!(
            "final f = File('{d}/raf.bin');
             f.writeAsStringSync('');
             final raf = f.openSync(mode: FileMode.write);
             raf.writeStringSync('abc');
             raf.flushSync();
             raf.positionSync();",
        ),
        "raf.positionSync()",
    );
    assert_eq!(num(&v), 3);
}

#[test]
fn socket_raw_options_roundtrip() {
    let mut c = permissive_ctx();
    let v = r#async(
        &mut c,
        "final a = await RawDatagramSocket.bind('127.0.0.1', 0);
         a.setRawOption(RawSocketOption.fromInt(RawSocketOption.levelTcp, 1, 42));
         final opt = a.getRawOption(RawSocketOption.levelTcp, 1);
         a.setOption(SocketOption.tcpNoDelay, true);
         final tcp = a.getRawOption(RawSocketOption.levelSocket, 1);
         final first = opt.value.isNotEmpty ? opt.value.first : -1;
         final tcpFirst = tcp.value.isNotEmpty ? tcp.value.first : -1;
         a.close();
         '$first|$tcpFirst';",
        "'$first|$tcpFirst'",
    );
    assert_eq!(str(&v), "42|1");
}

#[test]
fn socket_multicast_join_leave() {
    let mut c = permissive_ctx();
    let v = r#async(
        &mut c,
        "final a = await RawDatagramSocket.bind('127.0.0.1', 0);
         final group = InternetAddress('239.1.2.3');
         await a.joinMulticast(group);
         await a.leaveMulticast(group);
         a.close();
         'ok';",
        "'ok'",
    );
    assert_eq!(str(&v), "ok");
}
