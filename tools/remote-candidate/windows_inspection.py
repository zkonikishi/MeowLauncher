"""Use Windows PowerShell's own module discovery, independent of a pwsh 7 parent."""
import os

def inspection_environment(source=None):
    env = dict(os.environ if source is None else source)
    # Environment variable names are case-insensitive on Windows. Do not drop
    # application parameters, authentication settings or any other environment.
    for key in list(env):
        if key.casefold() == "psmodulepath":
            del env[key]
    return env
