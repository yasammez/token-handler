# Token Handler

This project is a proxy between a OAuth2 enabled identity provider, an API and a client. The main idea is to prevent
sensitive information, i.e. access and refresh tokens, to end up in an insecure environment, i.e. a web browser. To this
end, the complete login flow gets proxied by the token handler, which will then set an encrypted cookie. All API calls
go through the token handler as well, which decodes the cookie and proxies the request to the correct backend. To obtain
scalability and a predictable footprint, the token handler is completely stateless: all the necessary information is
encoded in the session cookie.

## Supported authentication flows

As of now, only the Authorization Code Flow with PKCE and client secret are supported. The client secret is sensitive
too and should never end up in code running on end user hardware.

## Prerequisites

* The IDP **must** have a configured client which has Authorization Code Flow with PKCE enabled and which is confidental
  (i.e. has a set client secret).
* The URL of the token handler deployment **must** be among the valid redirect URIs of said client.

## Configuration

To configure the token handler, point it to a `config.hcl` file (an example is provided in this repo):

```bash
cargo run -- -f config.hcl
```

The file has the following structure:

```hcl
port = 11001
expose_errors = true
clock_skew = 60

key "1" {
  value = "${KEY}"
  active = true
}

bridge "b1" {
  idp = "https://localhost:8080/auth/realms/my-realm"
  client = "client-id"
  secret = "ca00dd33-e4b2-4b11-93e2-093f5d145bbb"
  scope = "openid profile email"

  api "api" {
    backend = "http://localhost:11000/api"
    headers = [ "content-type", "if-match" ]
  }
}
```

Therein we find the following keys

* **port**: the TCP port the token handler will bind to (default 8080)
* **expose_errors**: Whether to report details about errors to the client. This is mostly useful for debugging and its
  use in production is discouraged (default false)
* **clock_skew**: Minimal time in seconds an access token needs to still be valid for without getting refreshed (default
  30)
* **key**: Cryptographic key. For an in-depth explanation, cf. below.
* **bridge**: A bridge is an abstraction for a single IDP/client connection. If you need to connect to multiple IDPs or
  configure multiple clients for one IDP, use a bridge for each.
* **bridge.idp**: Endpoint for the IDP. Notice that in this example a typical Keycloak URL is given, but any IDP that
  speaks proper OAuth2 will suffice.
* **bridge.client**: Name of the client to use with this IDP. This is the identifier that will be sent to the IDP for
  token requests. The name of the bridge is purely internal to the token handler (but *will* influence public facing URL
  paths).
* **bridge.secret**: The client secret for the client. This will remain confidental between the token handler and the
  IDP. Frontend could **should not** receive this.
* **bridge.scope**: A space-separated list of scopes to include in the token request (default "openid").
* **bridge.api**: This defines a backend API that will be proxied toward. A bridge can have an arbitrary number of APIs
  configured. They will all use the access tokens created by the bridge configuration.
* **bridge.api.backend**: URL of the API backend.
* **bridge.api.headers**: List of request headers that will be forwarded from proxied requests to the API (default [
  "content-type" ]).

## Cryptographic Keys

The token handler manages its state by issuing first party cookies to clients. To protect them from prying eyes, they're
symmetrically encrypted using AES. Every cookie contains an identifier for the used key, so that key rotation is
possible. A key can have any name (although that name needs to be stable for its entire lifecycle) and consists of:

* **value**: 32, preferrably random, bytes in Base64, for instance `TnVyIGVpbiBCZWlzcGllbCwgbmljaHQgYmVudXR6ZW4=`. As
  can be seen in the example, values can be sourced from environment variables. This allows a user to put the
  configuration file into a globally readable place apart from the actual secrets.
* **active**: whether this key is eligible for the creation of new cookies. When this is set to false, cookies with this
  key can still be used, but will be phased out. This is useful for key rotation (default false).


## Integration

Once the token handler is up and running, it can be used by clients. For this example, we assume an SPA written in
Angular. In the following we will assume

* A token handler running under `https://th.example.com` exposing a bridge `b1` and under that bridge an API
  `api`.

### Frontend

#### Login

In order to send the user to the login page and obtain a session cookie, some plumbing is required.

**app.module.ts**
```typescript

export const TOKEN_HANDLER_URL = new InjectionToken<string>('tokenHandlerUrl');

const initializeOauth = (tokenHandlerUrl: string) => () =>
    /*eslint-disable no-async-promise-executor*/
    new Promise(async resolve => {
        const resp = await fetch(tokenHandlerUrl + '/me', { credentials: 'include' });
        if (resp.status === 401) {
            window.location.href = `${tokenHandlerUrl}/login?redirect=${window.location.href}`;
        } else {
            (window as any).id_token = await resp.json();
            resolve(true);
        }
    });

@NgModule({
  providers: [
    {
      provide: TOKEN_HANDLER_URL,
      useValue: 'https://th.example.com/bridge/b1',
    },
    {
      provide: APP_INITIALIZER,
      useFactory: initializeOauth,
      multi: true,
      deps: [TOKEN_HANDLER_URL],
    },
  ]
})
export class AppModule {}
```

The result of the above code should be a logged in session with the OAuth2 IdToken in a global variable.

#### API calls

To call the backend API, just configure the respective token handler base path:

**app.module.ts**
```typescript
@NgModule({
  providers: [
    {
      provide: ApiConfiguration,
      useFactory: (tokenHandlerUrl: string) =>
        new ApiConfiguration({
          basePath: `${tokenHandlerUrl}/proxy/api`,
          withCredentials: true,
        }),
        deps: [TOKEN_HANDLER_URL],
        multi: false,
    }
  ],
export class AppModule {}
```

#### Logout

Redirect the user to the logout URL of the token handler.

**logout.component.html**
```html
<a href="https://th.example.com/bridge/b1/logout">Logout</a>
```

You can also include a query parameter `post_logout_redirect_uri` which **must** be configured as a valid post logout
redirect URI in the OAuth2 client. Following the logout, the user agent will be redirected to that page. When this
parameter is omitted, the referer is used instead.

### Backend

No extra steps are required to integrate an already OAuth2 enabled API!

## Endpoints

There exists one global endpoint

* **GET /health**: Always answers `up`. This can be used for k8s liveness and/or readiness checks.

Also, every configured bridge exposes the following endpoints:

* **GET /bridge/{bridgeId}/me**: Checks, if the user is already logged in to this bridge. If so, a JSON object
  containing the OAuth2 IdToken will be returned. This can be used to extract displayable information like a user name
  or email address. Otherwise, the token handler answers with HTTP 401 which inddicates that a login should be
  attempted.
* **GET /bridge/{bridgeId}/login**: Initiates the login flow. If the user is already authenticated with the bridge's
  IDP, this can short-circuit to an SSO login, which should be transparent.
* **GET /bridge/{bridgeId}/login2**: This is the callback address for the login, once the IDP is satisfied. There is no
  need to call this endpoint manually.
* **GET /bridge/{bridgeId}/logout**: This sends the user agent to the IDP and indicates that a logout is requested.

For every bridge, every configured API provides a proxying endpoint:

* **{METHOD} /bridge/{bridgeId}/proxy/{api}/...**: This proxies the request to the configured backend, together with all
  remaining path segments and parameters, as well as the configured headers. An `Authorization` header will be included
  with the access token from the session cookie. If the token has expired, it will be transparently refreshed using the
  refresh token and the cookie will be updated. Apart from that, the response gets forwarded back to the caller
  verbatim.


[modeline]: # ( vim: set textwidth=120 cc=120 :)
