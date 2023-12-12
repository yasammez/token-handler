# Port to listen to; default 8080
port = 11001

# Whether to report details about errors to the client; default false
expose_errors = true

# Cryptographic keys for cookies in base64.
key "1" {
  # Like everything in this file, this can be templated from environment variables.
  value = "${KEY}"
  # Use this key to create new cookies; cookies created with inactive keys will be phased out over time. This is useful
  # for zero downtime key rotation; default false
  active = true
}

key "2" {
  value = "${KEY_2}"
}

# Minimal time in seconds an access token needs to still be valid for without getting refreshed; default 30
clock_skew = 60

# A token handler can have an arbitrary number of bridges. This bridge will have endpoints
# * /bridge/b1/login
# * /bridge/b1/login2
# * /bridge/b1/logout
# * /bridge/b1/logout?post_logout_redirect_uri=...
# * /bridge/b1/me
# * /bridge/b1/proxy/... (see below)
bridge "b1" {
  # uri of the identity provider for this bridge
  idp = "https://localhost:8080/auth/realms/my-realm"

  # OAuth client to use with this bridge
  client = "client-id"

  # OAuth client secret
  secret = "ca00dd33-e4b2-4b11-93e2-093f5d145bbb"

  # scopes to request; default "openid"
  scope = "openid profile email"

  # each bridge can route an arbitrary number of backends. This one will be available under /bridge/b1/proxy/api/**
  api "api" {
    # uri where the real backend can be found
    backend = "http://localhost:11000/api"

    # list of http headers to proxy forward to the API; default [ "content-type" ]
    headers = [ "content-type", "if-match" ]
  }
}
