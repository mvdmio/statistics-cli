# Statistics CLI

The `statistics` command-line tool for the Statistics REST API, and the tests that prove it works against Statistics.

## Language

### Testing against Statistics

**Fake host**:
A stand-in for Statistics or Auth that a test starts inside its own process and scripts request by request. It is used by every test.
_Avoid_: Mock server, stub
