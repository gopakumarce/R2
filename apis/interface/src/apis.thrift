struct ScApi {
  1: i32 m1,
  2: i32 d,
  3: i32 m2,
}

struct CurvesApi {
    1: optional ScApi r_sc,
    2: optional ScApi u_sc,
    3: ScApi f_sc,
}

exception InterfaceErr {
  1: string why
}

service Interface {
    void add_if(1:string ifname, 2:i32 ifindex, 3:string mac) throws (1:InterfaceErr ouch),
    void add_ip(1:string ifname, 2:string ip_and_mask) throws (1:InterfaceErr ouch),
    void add_class(1:string ifname, 2:string name, 3:string parent, 4:i32 qlimit, 5:bool is_leaf, 6:CurvesApi curves) throws (1:InterfaceErr ouch)
}