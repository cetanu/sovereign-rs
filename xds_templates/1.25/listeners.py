import json
import envoy_data_plane.envoy.api.v2 as envoy


route_config = envoy.RouteConfiguration(
    name="MyRouteConfig",
    virtual_hosts=[
        envoy.route.VirtualHost(
            name="SomeWebsite",
            domains=["foobar.com"],
            routes=[
                envoy.route.Route(
                    name="catchall",
                    match=envoy.route.RouteMatch(prefix="/"),
                    direct_response=envoy.route.DirectResponseAction(
                        status=200,
                        body=envoy.core.DataSource(inline_string="Hello there"),
                    ),
                )
            ],
        )
    ],
)


def call(things):
    args = json.loads(things)
    print(args.keys())
    return json.dumps([route_config.to_dict()])
