"""Forge's unittest runner: discovers or loads tests and reports them as TeamCity service
messages on stdout, which Forge turns into the Run tool window's test tree.

    --discover DIR --pattern GLOB   discover the tests below DIR (default: . and test*.py)
    --file PATH [--test NAME]       the tests of a file (NAME: Class or Class.method)
    --id MODULE.Class.method        one test (repeatable; Rerun Failed Tests)
    --coverage-xml FILE             measure coverage with coverage.py (branches included)
                                    and write a Cobertura report (Run with Coverage)

Python 3.7+, standard library only (coverage.py for --coverage-xml).
"""
import inspect
import os
import pprint
import sys
import time
import traceback
import unittest

OUT = sys.stdout
__unittest = True  # unittest leaves frames of this module out of tracebacks
CWD = os.getcwd()


def esc(s):
    return (
        str(s)
        .replace("|", "||")
        .replace("'", "|'")
        .replace("\n", "|n")
        .replace("\r", "|r")
        .replace("[", "|[")
        .replace("]", "|]")
    )


def msg(kind, **attrs):
    parts = " ".join("%s='%s'" % (k, esc(v)) for k, v in attrs.items() if v is not None)
    OUT.write("##teamcity[%s %s]\n" % (kind, parts))
    OUT.flush()


def rel(path):
    try:
        r = os.path.relpath(path, CWD)
        return path if r.startswith("..") else r.replace(os.sep, "/")
    except ValueError:
        return path


def text_of(value):
    if isinstance(value, str):
        return value
    return pprint.pformat(value, width=100)


# assertEqual's values, for "Click to see difference".
_assert_equal = unittest.TestCase.assertEqual


def _recording_assert_equal(self, first, second, msg=None):
    try:
        _assert_equal(self, first, second, msg)
    except AssertionError as e:
        e.forge_values = (text_of(second), text_of(first))
        raise


unittest.TestCase.assertEqual = _recording_assert_equal
unittest.TestCase.assertEquals = _recording_assert_equal


def location(test):
    """(file, line) of a test method, or of its class."""
    cls = type(test)
    method = getattr(test, "_testMethodName", None)
    try:
        target = getattr(cls, method) if method else cls
        target = inspect.unwrap(target)
        path = inspect.getsourcefile(target) or inspect.getsourcefile(cls)
        line = inspect.getsourcelines(target)[1]
        return rel(path), line
    except (TypeError, OSError, AttributeError):
        return None, None


def flatten(suite):
    for t in suite:
        if isinstance(t, unittest.TestSuite):
            for x in flatten(t):
                yield x
        else:
            yield t


class Result(unittest.TestResult):
    def __init__(self):
        unittest.TestResult.__init__(self)
        self.started = {}
        self.module = None
        self.cls = None

    def suites(self, test):
        cls = type(test)
        module = cls.__module__
        name = cls.__name__
        if module != self.module:
            self.close_class()
            if self.module is not None:
                msg("testSuiteFinished", name=self.module)
            self.module = module
            path, _ = location(test)
            msg("testSuiteStarted", name=module, locationHint="python://%s" % path if path else None)
        if name != self.cls:
            self.close_class()
            self.cls = name
            path, line = None, None
            try:
                path = rel(inspect.getsourcefile(cls))
                line = inspect.getsourcelines(cls)[1]
            except (TypeError, OSError):
                pass
            hint = "python://%s:%s::%s" % (path, line, name) if path else None
            msg("testSuiteStarted", name=name, locationHint=hint)

    def close_class(self):
        if self.cls is not None:
            msg("testSuiteFinished", name=self.cls)
            self.cls = None

    def finish(self):
        self.close_class()
        if self.module is not None:
            msg("testSuiteFinished", name=self.module)
            self.module = None

    def name(self, test):
        return getattr(test, "_testMethodName", None) or str(test)

    def startTest(self, test):
        unittest.TestResult.startTest(self, test)
        self.suites(test)
        self.started[test] = time.time()
        path, line = location(test)
        hint = "python://%s:%s::%s" % (path, line, self.name(test)) if path else None
        msg("testStarted", name=self.name(test), locationHint=hint)

    def stopTest(self, test):
        unittest.TestResult.stopTest(self, test)
        ms = int((time.time() - self.started.pop(test, time.time())) * 1000)
        msg("testFinished", name=self.name(test), duration=ms)

    def fail(self, test, err, prefix=""):
        exc = err[1]
        details = self._exc_info_to_string(err, test)
        message = prefix + (str(exc).split("\n")[0] if str(exc) else type(exc).__name__)
        values = getattr(exc, "forge_values", None)
        if values:
            msg("testFailed", name=self.name(test), message=message, details=details,
                type="comparisonFailure", expected=values[0], actual=values[1])
        else:
            msg("testFailed", name=self.name(test), message=message, details=details)

    def addFailure(self, test, err):
        unittest.TestResult.addFailure(self, test, err)
        self.fail(test, err)

    def addError(self, test, err):
        unittest.TestResult.addError(self, test, err)
        if not hasattr(test, "_testMethodName"):
            # A module that failed to import, a failing setUpClass: report it as a test.
            name = str(test)
            msg("testStarted", name=name)
            msg("testFailed", name=name, message=str(err[1]), details=self._exc_info_to_string(err, test))
            msg("testFinished", name=name)
            return
        self.fail(test, err, "%s: " % type(err[1]).__name__)

    def addSubTest(self, test, subtest, err):
        unittest.TestResult.addSubTest(self, test, subtest, err)
        if err is not None:
            self.fail(test, err, "%s: " % subtest._subDescription())

    def addSkip(self, test, reason):
        unittest.TestResult.addSkip(self, test, reason)
        msg("testIgnored", name=self.name(test), message=reason)

    def addUnexpectedSuccess(self, test):
        unittest.TestResult.addUnexpectedSuccess(self, test)
        msg("testFailed", name=self.name(test), message="Unexpected success")


def module_of(path):
    r = rel(os.path.abspath(path))
    if r.endswith(".py"):
        r = r[:-3]
    parts = [p for p in r.replace("\\", "/").split("/") if p]
    if parts and parts[-1] == "__init__":
        parts.pop()
    return ".".join(parts)


def load_file(loader, path, test):
    name = module_of(path)
    target = name + ("." + test if test else "")
    try:
        __import__(name)
    except ImportError:
        # Not importable from the working directory (src layouts): import it by its folder.
        sys.path.insert(0, os.path.dirname(os.path.abspath(path)))
        name = os.path.splitext(os.path.basename(path))[0]
        target = name + ("." + test if test else "")
    return loader.loadTestsFromName(target)


def main(argv):
    loader = unittest.TestLoader()
    start, pattern, path, test, ids, coverage_xml = None, "test*.py", None, None, [], None
    i = 0
    while i < len(argv):
        a = argv[i]
        value = argv[i + 1] if i + 1 < len(argv) else None
        if a == "--discover":
            start = value
        elif a == "--pattern":
            pattern = value or pattern
        elif a == "--file":
            path = value
        elif a == "--test":
            test = value
        elif a == "--id":
            ids.append(value)
        elif a == "--coverage-xml":
            coverage_xml = value
        else:
            i += 1
            continue
        i += 2
    if "" not in sys.path and CWD not in sys.path:
        sys.path.insert(0, CWD)
    cov = start_coverage() if coverage_xml else None
    try:
        return run(loader, start, pattern, path, test, ids)
    finally:
        if cov is not None:
            cov.stop()
            cov.xml_report(outfile=coverage_xml)


def start_coverage():
    """Measure from here on, before the tests' modules are imported."""
    try:
        import coverage
    except ImportError:
        sys.stderr.write("Run with Coverage needs coverage.py in this interpreter: pip install coverage\n")
        return None
    cov = coverage.Coverage(data_file=None, branch=True, omit=["*forge_unittest.py"])
    cov.start()
    return cov


def run(loader, start, pattern, path, test, ids):
    if ids:
        suite = loader.loadTestsFromNames(ids)
    elif path:
        suite = load_file(loader, path, test)
    else:
        suite = loader.discover(start or ".", pattern=pattern, top_level_dir=CWD)
    tests = list(flatten(suite))
    msg("testCount", count=len(tests))
    result = Result()
    unittest.registerResult(result)
    start_time = time.time()
    suite.run(result)
    result.finish()
    took = time.time() - start_time
    sys.stderr.write("\nRan %d test%s in %.3fs\n" % (result.testsRun, "" if result.testsRun == 1 else "s", took))
    sys.stderr.write("\n%s\n" % ("OK" if result.wasSuccessful() else "FAILED (failures=%d, errors=%d)" % (len(result.failures), len(result.errors))))
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    try:
        code = main(sys.argv[1:])
    except Exception:
        traceback.print_exc()
        code = 2
    sys.exit(code)
