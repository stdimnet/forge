/**
 * A few PHP stubs in phpstorm-stubs' layout (one folder per extension) for browser mode:
 * Go to Declaration on `\Throwable`, `RuntimeException` or `array_map` opens them read-only
 * as `library://php-stubs/<path>`, like the desktop app does with the downloaded stubs.
 * Abbreviated: only what the demo project uses.
 */
export const demoStubsDetail = "JetBrains phpstorm-stubs 2025.3 (Apache-2.0)";

export const demoStubs: Record<string, string> = {
  "Core/Core_c.php": `<?php

use JetBrains\\PhpStorm\\Internal\\LanguageLevelTypeAware;
use JetBrains\\PhpStorm\\Pure;

/**
 * Created by typecasting to object.
 */
class stdClass {}

/**
 * Throwable is the base interface for any object that can be thrown via a throw statement
 * in PHP 7, including Error and Exception.
 * @link https://php.net/manual/en/class.throwable.php
 * @since 7.0
 */
interface Throwable extends Stringable
{
    /**
     * Gets the message
     * @link https://php.net/manual/en/throwable.getmessage.php
     * @return string
     * @since 7.0
     */
    public function getMessage(): string;

    /**
     * Gets the exception code
     * @return int
     * @since 7.0
     */
    public function getCode();

    /**
     * Gets the file in which the exception occurred
     * @return string Returns the name of the file from which the object was thrown.
     * @since 7.0
     */
    public function getFile(): string;

    /**
     * Gets the line on which the object was instantiated
     * @return int Returns the line number where the thrown object was instantiated.
     * @since 7.0
     */
    public function getLine(): int;

    /**
     * Gets the stack trace
     * @return array Returns the stack trace as an array in the same format as getTrace()
     * @since 7.0
     */
    public function getTrace(): array;

    /**
     * Returns the previous Throwable
     * @return null|Throwable Returns the previous Throwable if available, or NULL otherwise.
     * @since 7.0
     */
    public function getPrevious(): ?Throwable;
}

/**
 * Exception is the base class for all Exceptions.
 * @link https://php.net/manual/en/class.exception.php
 */
class Exception implements Throwable
{
    /** The error message */
    protected $message;

    /** The error code */
    protected $code;

    /**
     * Construct the exception. Note: The message is NOT binary safe.
     * @param string $message [optional] The Exception message to throw.
     * @param int $code [optional] The Exception code.
     * @param null|Throwable $previous [optional] The previous throwable used for the exception chaining.
     */
    #[Pure]
    public function __construct(string $message = "", int $code = 0, ?Throwable $previous = null) {}

    /**
     * Gets the Exception message
     * @return string the Exception message as a string.
     */
    #[Pure]
    final public function getMessage(): string {}

    /**
     * Gets the Exception code
     * @return mixed|int the exception code as integer.
     */
    #[Pure]
    final public function getCode() {}

    #[Pure]
    final public function getFile(): string {}

    #[Pure]
    final public function getLine(): int {}

    #[Pure]
    final public function getTrace(): array {}

    #[Pure]
    final public function getPrevious(): ?Throwable {}

    public function __toString(): string {}
}
`,
  "Core/Core_d.php": `<?php

define('E_ERROR', 1);
define('E_WARNING', 2);
define('E_ALL', 32767);
define('PHP_EOL', "\\n");
define('PHP_INT_MAX', 9223372036854775807);
`,
  "Core/Stringable.php": `<?php

/**
 * @since 8.0
 */
interface Stringable
{
    /**
     * Magic method {@see https://www.php.net/manual/en/language.oop5.magic.php#object.tostring}
     * allows a class to decide how it will react when it is treated like a string.
     *
     * @return string Returns string representation of the object that
     * implements this interface (and/or "__toString" magic method).
     */
    public function __toString(): string;
}
`,
  "SPL/SPL.php": `<?php

/**
 * Exception thrown if an error which can only be found on runtime occurs.
 * @link https://php.net/manual/en/class.runtimeexception.php
 */
class RuntimeException extends Exception {}

/**
 * Exception that represents error in the program logic. This kind of
 * exception should lead directly to a fix in your code.
 * @link https://php.net/manual/en/class.logicexception.php
 */
class LogicException extends Exception {}

/**
 * Exception thrown if an argument is not of the expected type.
 * @link https://php.net/manual/en/class.invalidargumentexception.php
 */
class InvalidArgumentException extends LogicException {}
`,
  "standard/standard_9.php": `<?php

use JetBrains\\PhpStorm\\Pure;

/**
 * Applies the callback to the elements of the given arrays
 * @link https://php.net/manual/en/function.array-map.php
 * @param callable|null $callback Callback function to run for each element in each array.
 * @param array $array An array to run through the callback function.
 * @param array ...$arrays
 * @return array an array containing all the elements of arr1
 * after applying the callback function to each one.
 */
function array_map(?callable $callback, array $array, array ...$arrays): array {}

/**
 * Return all the values of an array
 * @link https://php.net/manual/en/function.array-values.php
 * @param array $array The array.
 * @return array an indexed array of values.
 */
#[Pure]
function array_values(array $array): array {}

/**
 * Computes the difference of arrays
 * @link https://php.net/manual/en/function.array-diff.php
 * @param array $array The array to compare from
 * @param array ...$arrays An array to compare against
 * @return array an array containing all the entries from
 * array1 that are not present in any of the other arrays.
 */
#[Pure]
function array_diff(array $array, array ...$arrays): array {}
`,
  "standard/standard_3.php": `<?php

/**
 * Rounds a float
 * @link https://php.net/manual/en/function.round.php
 * @param int|float $num The value to round
 * @param int $precision [optional] The optional number of decimal digits to round to.
 * @param int $mode [optional] One of PHP_ROUND_HALF_UP, PHP_ROUND_HALF_DOWN, PHP_ROUND_HALF_EVEN, or PHP_ROUND_HALF_ODD.
 * @return float The rounded value
 */
#[Pure]
function round(int|float $num, int $precision = 0, int $mode = PHP_ROUND_HALF_UP): float {}
`,
};
