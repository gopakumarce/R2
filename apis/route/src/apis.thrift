
exception RouteErr {
  1: string why
}

service Route {
    void add_route(1:string ip_and_mask, 2:string nhop, 3:string ifname) throws (1:RouteErr ouch),
    void del_route(1:string ip_and_mask, 2:string nhop, 3:string ifname) throws (1:RouteErr ouch),
    string show(1:string prefix, 2:string filename) throws (1:RouteErr ouch),
}