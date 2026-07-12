# Generated clients

`python/` is the versioned `email-rs-client` package generated from the
schema-bearing OpenAPI contract in `email-server`.

Regenerate it after an API contract change:

```sh
./scripts/generate-python-client.sh
```

The package version follows the email-rs product/API version. Release tags
build and publish it to the `indexarr` Forgejo PyPI registry. Consumers should
pin an exact version and update it through an explicit dependency change.
