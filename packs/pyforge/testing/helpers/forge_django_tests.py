"""Forge's Django test runner: `manage.py test` with Django's own DiscoverRunner (test
database, settings, apps), reporting the results as TeamCity service messages through
Forge's unittest reporter (forge_unittest.py, passed in as FORGE_UNITTEST).

    --manage PATH   the project's manage.py (default: manage.py)
    the rest        test labels and options of `manage.py test`

Python 3.7+, standard library and Django only.
"""
import base64
import os
import runpy
import sys
import types
import unittest

_reporter = types.ModuleType("forge_unittest")
exec(compile(base64.b64decode(FORGE_UNITTEST).decode("utf-8"), "forge_unittest.py", "exec"), _reporter.__dict__)  # noqa: F821


def _make_runner():
    from django.test.runner import DiscoverRunner

    class ForgeTestRunner(DiscoverRunner):
        def run_suite(self, suite, **kwargs):
            _reporter.msg("testCount", count=len(list(_reporter.flatten(suite))))
            result = _reporter.Result()
            unittest.registerResult(result)
            suite.run(result)
            result.finish()
            return result

    return ForgeTestRunner


_runner_module = types.ModuleType("forge_django_tests")


def _runner_attr(name):
    # Created on first use: Django is importable (and configured) only by then.
    if name == "ForgeTestRunner":
        cls = _make_runner()
        _runner_module.ForgeTestRunner = cls
        return cls
    raise AttributeError(name)


_runner_module.__getattr__ = _runner_attr
sys.modules["forge_django_tests"] = _runner_module


def main(argv):
    manage = "manage.py"
    if argv[:1] == ["--manage"] and len(argv) > 1:
        manage, argv = argv[1], argv[2:]
    folder = os.path.dirname(os.path.abspath(manage))
    if folder not in sys.path:
        sys.path.insert(0, folder)
    sys.argv = [manage, "test", "--testrunner", "forge_django_tests.ForgeTestRunner"] + argv
    runpy.run_path(manage, run_name="__main__")


if __name__ == "__main__":
    main(sys.argv[1:])
