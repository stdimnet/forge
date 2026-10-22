/** Folders of the demo's include paths (Settings | PHP | Include Path) for browser mode. */
export const demoLibraryFiles: Record<string, string> = {
  "/usr/share/php/PEAR.php": `<?php

/**
 * Base class for other PEAR classes.
 */
class PEAR
{
    public static function isError($data, $code = null): bool
    {
        return $data instanceof PEAR_Error;
    }
}
`,
  "/usr/share/php/Console/Getopt.php": `<?php

class Console_Getopt
{
    public static function getopt(array $args, string $shortOptions, ?array $longOptions = null): array
    {
        return [];
    }
}
`,
  "/usr/share/php/Net/URL2.php": `<?php

class Net_URL2
{
    public function __construct(string $url, array $options = [])
    {
    }

    public function getNormalizedURL(): string
    {
        return '';
    }
}
`,
};

/** Whether `dir` (absolute) holds demo library files. */
export const demoLibraryExists = (dir: string): boolean => {
  const d = dir.replace(/\/+$/, "");
  return Object.keys(demoLibraryFiles).some((p) => p.startsWith(`${d}/`));
};
