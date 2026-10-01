//! Auto-extracted test cases from ~/d4rt/test/stdlib/async/async_test.dart.
//! Test names mirror the Dart originals, hence non-snake-case.
#![allow(non_snake_case)]
use d4rt_rs::Context;
use d4rt_rs::value::Value;

fn ev(c: &mut Context, src: &str) -> Value {
    c.eval(src).unwrap_or_else(|e| panic!("eval failed: {e}"))
}

fn is_list_1_8(v: &Value) -> bool {
    match v {
        Value::List(l) => {
            let b = l.borrow();
            let nums: Vec<i64> = b
                .iter()
                .map(|x| match x {
                    Value::Int(i) => *i,
                    _ => -1,
                })
                .collect();
            nums == vec![1, 2, 3, 4, 5, 6, 7, 8]
        }
        _ => false,
    }
}

#[test]
fn ref_await_Future_value() {
    let mut c = Context::new();
    let case = "Future<int> main() async {\n          int result = await Future.value(42);\n          return result;\n        }";
    assert_eq!(ev(&mut c, case), Value::Int(42));
}
#[test]
fn ref_await_Future_delayed() {
    let mut c = Context::new();
    let case = "Future<String> main() async {\n          await Future.delayed(Duration(milliseconds: 10));\n          return \"Done\";\n        }";
    assert_eq!(ev(&mut c, case), Value::str("Done"));
}
#[test]
fn ref_Assign_await_result_to_variable() {
    let mut c = Context::new();
    let case = "Future<int> fetchValue() async {\n           await Future.delayed(Duration(milliseconds: 5));\n           return 100;\n        }\n\n        Future<int> main() async {\n          int x = await fetchValue();\n          return x + 5;\n        }";
    assert_eq!(ev(&mut c, case), Value::Int(105));
}
#[test]
fn ref_Return_await_result_directly() {
    let mut c = Context::new();
    let case = "Future<String> fetchMessage() async {\n           await Future.delayed(Duration(milliseconds: 5));\n           return \"Returned Message\";\n        }\n\n        Future<String> main() async {\n          return await fetchMessage();\n        }";
    assert_eq!(ev(&mut c, case), Value::str("Returned Message"));
}
#[test]
fn ref_Multiple_awaits_in_sequence() {
    let mut c = Context::new();
    let case = "Future<int> step1() async {\n           await Future.delayed(Duration(milliseconds: 5));\n           return 10;\n        }\n        Future<int> step2(int prev) async {\n           await Future.delayed(Duration(milliseconds: 5));\n           return prev + 20;\n        }\n\n        Future<int> main() async {\n          int r1 = await step1();\n          int r2 = await step2(r1);\n          return r2 + 30;\n        }";
    assert_eq!(ev(&mut c, case), Value::Int(60));
}
#[test]
fn ref_Nested_async_function_calls() {
    let mut c = Context::new();
    let case = "Future<int> inner() async {\n           await Future.delayed(Duration(milliseconds: 1));\n           return 50;\n         }\n         Future<int> middle() async {\n            int val = await inner();\n            await Future.delayed(Duration(milliseconds: 1));\n            return val + 50;\n         }\n        Future<int> main() async {\n          int result = await middle();\n          return result + 50;\n        }";
    assert_eq!(ev(&mut c, case), Value::Int(150));
}
#[test]
fn ref_Sync_code_between_awaits() {
    let mut c = Context::new();
    let case = "Future<String> part1() async { return \"A\"; }\n        Future<String> part3() async { return \"C\"; }\n\n        Future<String> main() async {\n           String r1 = await part1();\n           String r2 = r1 + \"B\"; // Sync operation\n           String r3 = await part3();\n           return r2 + r3; // Sync operation\n        }";
    assert_eq!(ev(&mut c, case), Value::str("ABC"));
}
#[test]
fn ref_Return_non_Future_from_async_function() {
    let mut c = Context::new();
    let case = "Future<int> getValue() async {\n          // No await needed here\n          return 99;\n        }\n\n        Future<int> main() async {\n          return await getValue();\n        }";
    assert_eq!(ev(&mut c, case), Value::Int(99));
}
#[test]
fn ref_Interpreted_Async_Await_Tests_await_in_if_condition() {
    let mut c = Context::new();
    let case = "Future<bool> checkCondition() async {\n          await Future.delayed(Duration(milliseconds: 1));\n          return true;\n        }\n        Future<String> main() async {\n          String result = \"Initial\";\n          if (await checkCondition()) {\n            result = \"Condition True\";\n          }\n          return result;\n        }";
    assert_eq!(ev(&mut c, case), Value::str("Condition True"));
}
#[test]
fn ref_Interpreted_Async_Await_Tests_await_in_for_loop() {
    let mut c = Context::new();
    let case = "Future<int> processItem(int i) async {\n          await Future.delayed(Duration(milliseconds: 1));\n          return i * 10;\n        }\n        Future<int> main() async {\n          int total = 0;\n          for (int i = 0; i < 3; i++) {\n            total += await processItem(i);\n          }\n          return total; // 0*10 + 1*10 + 2*10 = 30\n        }";
    assert_eq!(ev(&mut c, case), Value::Int(30));
}
#[test]
fn ref_Interpreted_Async_Await_Tests_await_in_try_catch_success() {
    let mut c = Context::new();
    let case = "Future<String> successfulFuture() async {\n          await Future.delayed(Duration(milliseconds: 1));\n          return \"Success\";\n        }\n        Future<String> main() async {\n          String result = \"Failed\";\n          try {\n            result = await successfulFuture();\n          } catch (e) {\n             result = \"Caught Error\";\n          }\n          return result;\n        }";
    assert_eq!(ev(&mut c, case), Value::str("Success"));
}
#[test]
fn ref_Interpreted_Async_Await_Tests_await_Future_error_in_try_catch() {
    let mut c = Context::new();
    let case = "Future<String> failingFuture() async {\n          await Future.delayed(Duration(milliseconds: 1));\n          throw 'Future Failed';\n        }\n        Future<String> main() async {\n          String result = \"Initial\";\n          try {\n            result = await failingFuture();\n            result = \"Future Succeeded Unexpectedly\"; // Should not reach here\n          } catch (e) {\n             result = \"Caught: $e\";\n          }\n          return result;\n        }";
    assert_eq!(ev(&mut c, case), Value::str("Caught: Future Failed"));
}
#[test]
fn ref_Interpreted_Async_Await_Tests_await_Future_error_without_try_catch() {
    let mut c = Context::new();
    let case = "Future<void> failingFuture() async {\n          await Future.delayed(Duration(milliseconds: 1));\n          throw 'Deliberate Error';\n        }\n        Future<void> main() async {\n           await failingFuture();\n        }";
    let e = c.eval(case).unwrap_err();
    assert!(matches!(e, d4rt_rs::error::InterpError::Throw(_)), "{e}");
}
#[test]
fn ref_async_while_loop_with_await_in_body() {
    let mut c = Context::new();
    let case = "// Define a simple async function to introduce an await point\n        Future<void> waitABit() async {\n           await Future.value(null); // The simplest await possible\n        }\n\n        Future<int> counter(int limit) async {\n          var i = 0;\n          var sum = 0;\n          while (i < limit) {\n            sum = sum + i;\n            i = i + 1;\n            // Use our self-defined async function\n            await waitABit(); \n          }\n          return sum;\n        }\n        main() {\n          return counter(5); // Expect 0+1+2+3+4 = 10\n        }";
    assert_eq!(ev(&mut c, case), Value::Int(10));
}
#[test]
fn ref_async_while_loop_with_await_in_condition() {
    let mut c = Context::new();
    let case = "// Helper async function for condition\n        Future<bool> shouldContinue(int currentVal) async {\n          await Future.value(null); // Simulate async work\n          return currentVal < 3; // Condition based on value\n        }\n\n        Future<int> looper() async {\n          var i = 0;\n          while (await shouldContinue(i)) {\n            i = i + 1;\n          }\n          return i; // Expect i to be 3 when loop terminates\n        }\n\n        main() {\n          return looper();\n        }";
    assert_eq!(ev(&mut c, case), Value::Int(3));
}
#[test]
fn ref_async_do_while_loop_with_await_in_body() {
    let mut c = Context::new();
    let case = "Future<void> waitABit() async {\n          await Future.value(null);\n        }\n\n        Future<int> looper() async {\n          var i = 0;\n          var sum = 0;\n          do {\n            sum = sum + i;\n            await waitABit(); // Async operation in body\n            i = i + 1;\n          } while (i < 4); // Condition évaluée après le corps\n          // Le corps s'exécute pour i = 0, 1, 2, 3.\n          // La condition est vérifiée pour i=1, 2, 3 (true), puis i=4 (false)\n          // sum = 0 + 1 + 2 + 3 = 6\n          return sum;\n        }\n\n        main() {\n          return looper();\n        }";
    assert_eq!(ev(&mut c, case), Value::Int(6));
}
#[test]
fn ref_async_do_while_loop_with_await_in_condition() {
    let mut c = Context::new();
    let case = "Future<bool> shouldContinue(int currentVal) async {\n          await Future.value(null); // Simulate async work\n          return currentVal < 3; // Condition based on value\n        }\n\n        Future<int> looper() async {\n          var i = 0;\n          do {\n            i = i + 1;\n            // Corps exécuté pour i=1, i=2, i=3\n          } while (await shouldContinue(i)); // Condition async\n          // Vérifie shouldContinue(1) -> true\n          // Vérifie shouldContinue(2) -> true\n          // Vérifie shouldContinue(3) -> false\n          // La boucle termine, i = 3\n          return i;\n        }\n\n        main() {\n          return looper();\n        }";
    assert_eq!(ev(&mut c, case), Value::Int(3));
}
#[test]
fn ref_async_for_loop_with_await_in_body() {
    let mut c = Context::new();
    let case = "Future<void> waitABit() async {\n          await Future.value(null);\n        }\n\n        Future<int> looper() async {\n          var sum = 0;\n          for (var i = 0; i < 4; i = i + 1) {\n            sum = sum + i;\n            await waitABit(); // Async in body\n          }\n          // i = 0, sum = 0, await\n          // i = 1, sum = 1, await\n          // i = 2, sum = 3, await\n          // i = 3, sum = 6, await\n          // i = 4, condition false\n          return sum; // Expect 6\n        }\n\n        main() {\n          return looper();\n        }";
    assert_eq!(ev(&mut c, case), Value::Int(6));
}
#[test]
fn ref_async_for_loop_with_await_in_initializer() {
    let mut c = Context::new();
    let case = "Future<int> getStart() async {\n          await Future.delayed(Duration(milliseconds: 1)); // Simulate async work\n          return 1;\n        }\n\n        Future<int> looper() async {\n          var sum = 0;\n          for (var i = await getStart(); i < 4; i = i + 1) {\n            sum = sum + i;\n          }\n          // await -> i = 1\n          // i = 1, sum = 1\n          // i = 2, sum = 3\n          // i = 3, sum = 6\n          // i = 4, condition false\n          return sum; // Expect 6\n        }\n\n        main() {\n          return looper();\n        }";
    assert_eq!(ev(&mut c, case), Value::Int(6));
}
#[test]
fn ref_async_for_loop_with_await_assignment_in_body() {
    let mut c = Context::new();
    let case = "Future<int> getValue(int iteration) async {\n          await Future.delayed(Duration(milliseconds: 1));\n          return iteration * 10;\n        }\n\n        Future<int> looper() async {\n          int total = 0;\n          int lastAwaitedValue = -1;\n          for (int i = 0; i < 3; i++) {\n            int awaitedValue = await getValue(i); // Assign await result\n            lastAwaitedValue = await getValue(i);\n            total += i; // Some other work\n          }\n          // Expected: total = 0+1+2=3, lastAwaitedValue = getValue(2)=20\n          return total + lastAwaitedValue;\n        }\n\n        main() {\n          // Implicitly returns the result of looper()\n          return looper();\n        }";
    assert_eq!(ev(&mut c, case), Value::Int(23));
}
#[test]
fn ref_async_for_loop_with_await_in_assignment_operator() {
    let mut c = Context::new();
    let case = "Future<int> getValue(int iteration) async {\n          await Future.delayed(Duration(milliseconds: 1));\n          return iteration * 10;\n        }\n\n        Future<int> looper() async {\n          int total = 0;\n          for (int i = 0; i < 3; i++) {\n            total += await getValue(i); // Assignment operation with await\n          }\n          // i=0: total += await getValue(0) => total += 0  => total = 0\n          // i=1: total += await getValue(1) => total += 10 => total = 10\n          // i=2: total += await getValue(2) => total += 20 => total = 30\n          return total;\n        }\n\n        main() {\n          return looper();\n        }";
    assert_eq!(ev(&mut c, case), Value::Int(30));
}
#[test]
fn ref_async_for_in_loop_with_await_in_body() {
    let mut c = Context::new();
    let case = "Future<int> getValue(int iteration) async {\n          await Future.delayed(Duration(milliseconds: 1)); // Simulate async work\n          final result = iteration * 10;\n          return result;\n        }\n\n        Future<int> testAsyncForInWithAwaitInBody() async {\n          final syncList = [1, 2, 3];\n          var total = 0;\n          for (var item in syncList) {\n            var awaitedValue = await getValue(item);\n            total += awaitedValue;\n            await Future.delayed(Duration(milliseconds: 1)); // Another await\n          }\n          return total;\n        }\n\n        Future<void> main() async {\n          await testAsyncForInWithAwaitInBody();\n        }";
    assert_eq!(ev(&mut c, case), Value::Int(60));
}
#[test]
fn ref_async_try_catch_finally_with_await_throwing_error() {
    let mut c = Context::new();
    let case = "Future<String> operationThatThrows() async {\n          await Future.delayed(Duration(milliseconds: 1));\n          throw Exception('Something went wrong asynchronously');\n          // Unreachable\n          // return \"Success\";\n        }\n\n        Future<String> testTryCatch() async {\n          String status = \"Initial\";\n          String finallyStatus = \"Not Executed\";\n          try {\n            status = \"In Try\";\n            await operationThatThrows();\n            status = \"Try Completed (Should not happen)\";\n          } catch (e) {\n            status = \"Caught Error\";\n          } finally {\n            finallyStatus = \"Finally Executed\";\n          }\n          return \"$status:$finallyStatus\";\n        }\n\n        Future<void> main() async {\n          await testTryCatch();\n        }";
    assert_eq!(
        ev(&mut c, case),
        Value::str("Caught Error:Finally Executed")
    );
}
#[test]
fn ref_async_if_statement_with_await_in_condition() {
    let mut c = Context::new();
    let case = "Future<bool> checkCondition(bool value) async {\n          await Future.delayed(Duration(milliseconds: 1));\n          return value;\n        }\n\n        Future<String> main() async {\n          var result = 'Initial';\n          if (await checkCondition(true)) {\n            result = 'First If True';\n          } else {\n            result = 'First If False (Error)';\n          }\n\n          if (await checkCondition(false)) {\n            result = 'Second If True (Error)';\n          } else {\n            result = 'Second If False';\n          }\n          return result;\n        }";
    assert_eq!(ev(&mut c, case), Value::str("Second If False"));
}
#[test]
fn ref_async_try_catch_with_rethrow_and_await_error() {
    let mut c = Context::new();
    let case = "Future<String> operationThatThrows() async {\n          await Future.delayed(Duration(milliseconds: 1));\n          throw Exception('Something went wrong asynchronously');\n          // Unreachable\n          // return \"Success\";\n        }\n\n        Future<String> testTryCatch() async {\n          String status = \"Initial\";\n          try {\n            status = \"In Try\";\n            await operationThatThrows();\n            status = \"Try Completed (Should not happen)\";\n          } catch (e) {\n            rethrow;\n          }\n          return \"ok\"; \n        }\n\n        Future<void> main() async {\n          await testTryCatch();\n        }";
    let e = c.eval(case).unwrap_err();
    assert!(matches!(e, d4rt_rs::error::InterpError::Throw(_)), "{e}");
}
#[test]
fn ref_nested_for_in_loops_with_list_processing() {
    let mut c = Context::new();
    let case = "main() async {\n        List resultList = [];\n        for (var element in [\n        [1, 2, 3, 4],\n        [5, 6, 7, 8]\n        ]) {\n        // Introduce a local variable for the current element\n        var currentElement = element;\n        for (var i = 0; i < currentElement.length; i++) {\n          resultList.add(currentElement[i]);\n        }\n        }\n        return resultList;\n      }";
    assert!(is_list_1_8(&ev(&mut c, case)));
}
#[test]
fn ref_nested_for_loops_with_list_processing_using_indexed_access() {
    let mut c = Context::new();
    let case = "main() async {\n        List resultList = [];\n        final dddd = [\n        [1, 2, 3, 4],\n        [5, 6, 7, 8]\n        ];\n        for (var i = 0; i < dddd.length; i++) {\n        var currentElement = dddd[i];\n        for (var j = 0; j < currentElement.length; j++) {\n          resultList.add(currentElement[j]);\n        }\n        }\n        return resultList;\n      }";
    assert!(is_list_1_8(&ev(&mut c, case)));
}
