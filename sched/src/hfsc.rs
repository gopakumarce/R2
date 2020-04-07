use common::time_nsecs;
use msg::{Curves, Sc};
use packet::BoxPkt;
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, VecDeque};

const SM_SHIFT: usize = 24;
const ISM_SHIFT: usize = 10;
const SM_MASK: u64 = (1 << SM_SHIFT) - 1;
const ISM_MASK: u64 = (1 << ISM_SHIFT) - 1;
const HT_INFINITY: u64 = std::u64::MAX;
const HFSC_FREQ: u64 = 1_000_000_000;

struct Key {
    time: u64,
    index: usize,
}

// We cant order on time alone because BtreeMap does not allow
// duplicate keys, and there can be many classes with the same time
impl Ord for Key {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.index == other.index {
            assert_eq!(self.time, other.time);
            Ordering::Equal
        } else if self.time < other.time {
            Ordering::Less
        } else if self.time > other.time {
            Ordering::Greater
        } else if self.index < other.index {
            Ordering::Less
        } else {
            Ordering::Greater
        }
    }
}

impl PartialOrd for Key {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Key {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Key {}

#[derive(Copy, Clone, Default)]
struct InternalSc {
    sm1: u64,
    ism1: u64,
    dx: u64,
    dy: u64,
    sm2: u64,
    ism2: u64,
}

#[derive(Copy, Clone, Default)]
struct RuntimeSc {
    x: u64,
    y: u64,
    sm1: u64,
    ism1: u64,
    dx: u64,
    dy: u64,
    sm2: u64,
    ism2: u64,
}

fn seg_x2y(x: u64, sm: u64) -> u64 {
    /*
     * compute
     *  y = x * sm >> SM_SHIFT
     * but divide it for the upper and lower bits to avoid overflow
     */
    (x >> SM_SHIFT) * sm + (((x & SM_MASK) * sm) >> SM_SHIFT)
}

fn seg_y2x(y: u64, ism: u64) -> u64 {
    if y == 0 {
        0
    } else if ism == HT_INFINITY {
        HT_INFINITY
    } else {
        (y >> ISM_SHIFT) * ism + (((y & ISM_MASK) * ism) >> ISM_SHIFT)
    }
}

fn m2sm(m: u64) -> u64 {
    (m << SM_SHIFT) / 8 / HFSC_FREQ
}

fn m2ism(m: u64) -> u64 {
    if m == 0 {
        HT_INFINITY
    } else {
        (HFSC_FREQ << ISM_SHIFT) * 8 / m
    }
}

fn d2dx(d: usize) -> u64 {
    (d as u64 * HFSC_FREQ) / 1000
}

fn sc2isc(sc: &Sc) -> InternalSc {
    InternalSc {
        sm1: m2sm(sc.m1),
        ism1: m2ism(sc.m1),
        dx: d2dx(sc.d),
        dy: seg_x2y(d2dx(sc.d), m2sm(sc.m1)),
        sm2: m2sm(sc.m2),
        ism2: m2ism(sc.m2),
    }
}

/*
 * initialize the runtime service curve with the given internal
 * service curve starting at (x, y).
 */
fn runtime_init(isc: &InternalSc, x: u64, y: u64) -> RuntimeSc {
    RuntimeSc {
        x,
        y,
        sm1: isc.sm1,
        ism1: isc.ism1,
        dx: isc.dx,
        dy: isc.dy,
        sm2: isc.sm2,
        ism2: isc.ism2,
    }
}

fn rtsc_y2x(rtsc: &RuntimeSc, y: u64) -> u64 {
    if y < rtsc.y {
        rtsc.x
    } else if y <= rtsc.y + rtsc.dy {
        /* x belongs to the 1st segment */
        if rtsc.dy == 0 {
            rtsc.x + rtsc.dx
        } else {
            rtsc.x + seg_y2x(y - rtsc.y, rtsc.ism1)
        }
    } else {
        /* x belongs to the 2nd segment */
        rtsc.x + rtsc.dx + seg_y2x(y - rtsc.y - rtsc.dy, rtsc.ism2)
    }
}

fn rtsc_x2y(rtsc: &RuntimeSc, x: u64) -> u64 {
    if x <= rtsc.x {
        rtsc.y
    } else if x <= rtsc.x + rtsc.dx {
        /* y belongs to the 1st segment */
        rtsc.y + seg_x2y(x - rtsc.x, rtsc.sm1)
    } else {
        /* y belongs to the 2nd segment */
        rtsc.y + rtsc.dy + seg_x2y(x - rtsc.x - rtsc.dx, rtsc.sm2)
    }
}

fn rtsc_min(rtsc: &mut RuntimeSc, isc: &InternalSc, x: u64, y: u64) {
    if isc.sm1 <= isc.sm2 {
        /* service curve is convex */
        let y1 = rtsc_x2y(rtsc, x);
        if y1 < y {
            /* the current rtsc is smaller */
            return;
        }
        rtsc.x = x;
        rtsc.y = y;
        return;
    }

    /*
     * service curve is concave
     * compute the two y values of the current rtsc
     *  y1: at x
     *  y2: at (x + dx)
     */
    let y1 = rtsc_x2y(rtsc, x);
    if y1 <= y {
        /* rtsc is below isc, no change to rtsc */
        return;
    }

    let y2 = rtsc_x2y(rtsc, x + isc.dx);
    if y2 >= y + isc.dy {
        /* rtsc is above isc, replace rtsc by isc */
        rtsc.x = x;
        rtsc.y = y;
        rtsc.dx = isc.dx;
        rtsc.dy = isc.dy;
        return;
    }

    /*
     * the two curves intersect
     * compute the offsets (dx, dy) using the reverse
     * function of seg_x2y()
     *  seg_x2y(dx, sm1) == seg_x2y(dx, sm2) + (y1 - y)
     */
    let mut dx = ((y1 - y) << SM_SHIFT) / (isc.sm1 - isc.sm2);
    /*
     * check if (x, y1) belongs to the 1st segment of rtsc.
     * if so, add the offset.
     */
    if rtsc.x + rtsc.dx > x {
        dx += rtsc.x + rtsc.dx - x;
    }
    let dy = seg_x2y(dx, isc.sm1);

    rtsc.x = x;
    rtsc.y = y;
    rtsc.dx = dx;
    rtsc.dy = dy;
}

pub struct Hfsc {
    root: usize,
    free_index: VecDeque<usize>,
    eligible: BTreeMap<Key, usize>,
    classes: Vec<Class>,
    class_names: HashMap<String, usize>,
    get_time_ns: fn() -> u64,
    pkts_queued: usize,
}

impl Hfsc {
    pub fn new(bandwidth: usize) -> Self {
        let f_sc = Sc {
            m1: 0,
            d: 0,
            m2: bandwidth as u64,
        };
        let u_sc = Sc {
            m1: 0,
            d: 0,
            m2: bandwidth as u64,
        };
        let curves = Curves {
            r_sc: None,
            u_sc: Some(u_sc),
            f_sc,
        };
        let mut classes = Vec::new();
        let mut class_names = HashMap::new();
        let dummy = Class::dummy();
        classes.push(dummy);
        class_names.insert("dummy".to_string(), 0);

        let root = classes.len();
        let class = Class::new(0, root, 0, false, 0, curves);
        classes.push(class);
        class_names.insert("root".to_string(), root);

        Hfsc {
            root,
            free_index: VecDeque::new(),
            eligible: BTreeMap::new(),
            classes,
            class_names,
            get_time_ns: time_nsecs,
            pkts_queued: 0,
        }
    }

    pub fn pkts_queued(&self) -> usize {
        self.pkts_queued
    }

    pub fn has_classes(&self) -> bool {
        !self.classes[self.root].children.is_empty()
    }

    pub fn create_class(
        &mut self,
        name: String,
        parent_name: String,
        qlimit: usize,
        is_leaf: bool,
        curves: Curves,
    ) -> Result<(), String> {
        if self.class_names.get(&name).is_some() {
            return Err("Class already exits".to_string());
        }
        let parent;
        if let Some(p) = self.class_names.get(&parent_name) {
            parent = *p;
        } else {
            return Err("Parent not found".to_string());
        }
        if parent >= self.classes.len() || !self.classes[parent].in_use {
            return Err(format!("Invalid Parent {}", parent));
        }

        let pvoff = self.classes[parent].pvoff;
        if let Some(free) = self.free_index.pop_front() {
            let class = Class::new(parent, free, qlimit, is_leaf, pvoff, curves);
            self.classes[free] = class;
            self.class_names.insert(name, free);
        } else {
            let free = self.classes.len();
            let class = Class::new(parent, free, qlimit, is_leaf, pvoff, curves);
            self.classes.push(class);
            self.class_names.insert(name, free);
        }

        Ok(())
    }

    pub fn class_index(&self, name: String) -> Option<usize> {
        if let Some(idx) = self.class_names.get(&name) {
            Some(*idx)
        } else {
            None
        }
    }

    pub fn destroy_class(&mut self, index: usize) -> usize {
        if index >= self.classes.len() || !self.classes[index].in_use {
            return 0;
        }
        let class = &self.classes[index];
        let key = Key {
            time: class.eligible,
            index: class.index,
        };
        let is_empty = class.packets.is_empty();
        let is_realtime = class.r_isc.is_some();
        if !is_empty {
            self.update_v(index, 0, 0, true);
            if is_realtime {
                self.eligible.remove(&key);
            }
        }
        self.classes[index] = Class::dummy();
        self.free_index.push_back(index);
        index
    }

    fn get_min_d(&self, time: u64) -> usize {
        let mut deadline = std::u64::MAX;
        let mut class = 0;

        for (_key, child) in self.eligible.iter() {
            let c = &self.classes[*child];
            if c.eligible > time {
                break;
            }
            if c.deadline < deadline {
                class = *child;
                deadline = c.deadline;
            }
        }
        class
    }

    fn get_min_v(&mut self, parent: usize, time: u64) -> usize {
        let p = &self.classes[parent];
        let ch;
        if let Some((_key, child)) = p.children.iter().next() {
            ch = *child;
        } else {
            return 0;
        }

        assert_eq!(self.classes[ch].parent, parent);
        let vtime = self.classes[ch].vtime;
        let leaf = self.classes[ch].leaf;
        let mut p = &mut self.classes[parent];
        if vtime > p.vmin {
            p.vmin = vtime;
        }
        let r = self.get_min_v(ch, time);
        if r == 0 {
            if leaf {
                ch
            } else {
                0
            }
        } else {
            r
        }
    }

    fn update_v(&mut self, class: usize, len: usize, time: u64, passive: bool) {
        let key;
        let go_passive;
        let pvmin;
        let pindex;
        {
            let mut c = &mut self.classes[class];
            pindex = c.parent;
            if pindex == 0 {
                return;
            }
            c.f_bytes += len as u64;
            if c.nactive == 0 {
                self.update_v(pindex, len, time, passive);
                return;
            }

            if passive {
                c.nactive -= 1;
            }
            go_passive = passive && c.nactive == 0;
            key = Key {
                time: c.vtime,
                index: c.index,
            };
        }
        {
            let parent = &mut self.classes[pindex];
            assert_eq!(pindex, parent.index);
            pvmin = parent.vmin;
            parent.children.remove(&key);
        }
        if go_passive {
            let mut parent = &mut self.classes[pindex];
            if key.time > parent.vmax {
                parent.vmax = key.time;
            }
        } else {
            let key;
            {
                let c = &mut self.classes[class];
                c.vtime = rtsc_y2x(&c.f_run, c.f_bytes) - c.voff + c.vadj;
                if c.vtime < pvmin {
                    c.vadj += pvmin - c.vtime;
                    c.vtime = pvmin;
                }
                key = Key {
                    time: c.vtime,
                    index: class,
                };
            }
            let parent = &mut self.classes[pindex];
            parent.children.insert(key, class);
        }
        self.update_v(pindex, len, time, go_passive);
    }

    fn init_v(&mut self, class: usize, len: usize, active: bool) {
        let go_active;
        let pindex;
        {
            let mut c = &mut self.classes[class];
            pindex = c.parent;
            if pindex == 0 {
                return;
            }
            go_active = active && c.nactive == 0;
            if active {
                c.nactive += 1;
            }
        }

        if go_active {
            let mut max_child = 0;
            let mut max_vtime = 0;
            let pvmin;
            let pvperiod;
            let mut pvoff;
            let pnactive;
            {
                let parent = &self.classes[pindex];
                assert_eq!(parent.index, pindex);
                pvmin = parent.vmin;
                pvperiod = parent.vperiod;
                pvoff = parent.voff;
                pnactive = parent.nactive;
                if let Some((_key, cmax)) = parent.children.iter().next_back() {
                    max_child = *cmax;
                    max_vtime = self.classes[max_child].vtime;
                }
            }
            if max_child != 0 {
                let mut c = &mut self.classes[class];
                let mut vt = max_vtime;
                if pvmin != 0 {
                    vt = (pvmin + vt) / 2;
                }
                if pvperiod != c.pvperiod || vt > c.vtime {
                    c.vtime = vt;
                }
            } else {
                {
                    let parent = &mut self.classes[pindex];
                    parent.voff += parent.vmax;
                    pvoff = parent.voff;
                    parent.vmax = 0;
                    parent.vmin = 0;
                }
                {
                    let c = &mut self.classes[class];
                    c.vtime = 0;
                }
            }

            let key;
            {
                let c = &mut self.classes[class];
                c.voff = pvoff - c.pvoff;
                let vt = c.vtime + c.voff;
                rtsc_min(&mut c.f_run, &c.f_isc, vt, c.f_bytes);
                if c.f_run.x == vt {
                    c.f_run.x -= c.voff;
                    c.voff = 0;
                }
                c.vadj = 0;
                c.vperiod += 1;
                c.pvperiod = pvperiod;
                if pnactive == 0 {
                    c.pvperiod += 1;
                }
                key = Key {
                    time: c.vtime,
                    index: c.index,
                };
            }
            let parent = &mut self.classes[pindex];
            parent.children.insert(key, class);
            self.init_v(pindex, len, go_active);
        }
    }

    pub fn enqueue(&mut self, classid: usize, pkt: BoxPkt) -> bool {
        if classid >= self.classes.len() || !self.classes[classid].in_use {
            return false;
        }
        let qlen;
        {
            let mut c = &mut self.classes[classid];
            qlen = c.packets.len();
            if c.qlimit != 0 && qlen >= c.qlimit {
                c.qdrops += 1;
                return false;
            }
        }
        if qlen == 0 {
            self.init_v(classid, pkt.len(), true);
            let c = &mut self.classes[classid];
            if c.r_isc.is_some() {
                c.init_ed(pkt.len(), (self.get_time_ns)());
                let key = Key {
                    time: c.eligible,
                    index: c.index,
                };
                self.eligible.insert(key, classid);
            }
        }
        let c = &mut self.classes[classid];
        c.packets.push_back(pkt);
        self.pkts_queued += 1;
        true
    }

    pub fn dequeue(&mut self) -> Option<BoxPkt> {
        let time = (self.get_time_ns)();
        let mut ret = None;

        let child = self.get_min_d(time);
        if child != 0 {
            let mut c = &mut self.classes[child];
            if let Some(pkt) = c.packets.pop_front() {
                c.r_bytes += pkt.len() as u64;
                let qempty = c.packets.is_empty();
                if !qempty {
                    if c.r_isc.is_some() {
                        let next_len = c.packets.front().unwrap().len();
                        let key = Key {
                            time: c.eligible,
                            index: c.index,
                        };
                        self.eligible.remove(&key);
                        c.update_ed(next_len);
                        let key = Key {
                            time: c.eligible,
                            index: c.index,
                        };
                        self.eligible.insert(key, child);
                    }
                } else if c.r_isc.is_some() {
                    let key = Key {
                        time: c.eligible,
                        index: c.index,
                    };
                    self.eligible.remove(&key);
                }
                self.update_v(child, pkt.len(), time, qempty);
                ret = Some(pkt);
            }
        } else {
            let child = self.get_min_v(self.root, time);
            if child != 0 {
                let c = &mut self.classes[child];
                if let Some(pkt) = c.packets.pop_front() {
                    let qempty = c.packets.is_empty();
                    if !qempty {
                        if c.r_isc.is_some() {
                            let next_len = c.packets.front().unwrap().len();
                            c.update_d(next_len);
                        }
                    } else if c.r_isc.is_some() {
                        let key = Key {
                            time: c.eligible,
                            index: c.index,
                        };
                        self.eligible.remove(&key);
                    }
                    self.update_v(child, pkt.len(), time, qempty);
                    ret = Some(pkt);
                }
            }
        }
        if ret.is_some() {
            self.pkts_queued -= 1;
        }
        ret
    }
}

pub struct Class {
    in_use: bool,
    leaf: bool,
    parent: usize,
    index: usize,
    qlimit: usize,
    qdrops: usize,
    eligible: u64,
    deadline: u64,
    vtime: u64,
    vmin: u64,
    vmax: u64,
    voff: u64,
    pvoff: u64,
    vadj: u64,
    vperiod: u64,
    pvperiod: u64,
    f_bytes: u64,
    r_bytes: u64,
    f_isc: InternalSc,
    r_isc: Option<InternalSc>,
    u_isc: Option<InternalSc>,
    f_run: RuntimeSc,
    e_run: RuntimeSc,
    d_run: RuntimeSc,
    u_run: RuntimeSc,
    nactive: usize,
    children: BTreeMap<Key, usize>,
    packets: VecDeque<BoxPkt>,
}

impl Class {
    fn new(
        parent: usize,
        index: usize,
        qlimit: usize,
        is_leaf: bool,
        pvoff: u64,
        curves: Curves,
    ) -> Self {
        let mut r_isc = None;
        let mut e_run = Default::default();
        let mut d_run = Default::default();
        if let Some(r) = curves.r_sc {
            let r = sc2isc(&r);
            e_run = runtime_init(&r, 0, 0);
            d_run = runtime_init(&r, 0, 0);
            r_isc = Some(r);
        }

        let mut u_isc = None;
        let mut u_run = Default::default();
        if let Some(u) = curves.u_sc {
            let u = sc2isc(&u);
            u_run = runtime_init(&u, 0, 0);
            u_isc = Some(u);
        }

        let f_isc = sc2isc(&curves.f_sc);
        let f_run = runtime_init(&f_isc, 0, 0);

        Class {
            in_use: true,
            leaf: is_leaf,
            parent,
            index,
            qlimit,
            qdrops: 0,
            eligible: 0,
            deadline: 0,
            vtime: 0,
            vmin: 0,
            vmax: 0,
            voff: 0,
            pvoff,
            vadj: 0,
            vperiod: 0,
            pvperiod: 0,
            f_bytes: 0,
            r_bytes: 0,
            f_isc,
            r_isc,
            u_isc,
            f_run,
            e_run,
            d_run,
            u_run,
            nactive: 0,
            children: BTreeMap::new(),
            packets: VecDeque::new(),
        }
    }

    fn dummy() -> Self {
        let f_sc = Default::default();
        let curves = Curves {
            r_sc: None,
            u_sc: None,
            f_sc,
        };
        let mut class = Class::new(0, 0, 0, false, 0, curves);
        class.in_use = false;
        class
    }
    fn init_ed(&mut self, next_len: usize, cur_time: u64) {
        if let Some(ref r_isc) = self.r_isc {
            /* update the deadline curve */
            rtsc_min(&mut self.d_run, &r_isc, cur_time, self.f_bytes);

            /*
             * update the eligible curve.
             * for concave, it is equal to the deadline curve.
             * for convex, it is a linear curve with slope m2.
             */
            self.e_run = self.d_run;
            if r_isc.sm1 <= r_isc.sm2 {
                self.e_run.dx = 0;
                self.e_run.dy = 0;
            }

            /* compute e and d */
            self.eligible = rtsc_y2x(&self.e_run, self.f_bytes);
            self.deadline = rtsc_y2x(&self.d_run, self.f_bytes + next_len as u64);
        }
    }

    fn update_ed(&mut self, next_len: usize) {
        self.eligible = rtsc_y2x(&self.e_run, self.f_bytes);
        self.deadline = rtsc_y2x(&self.d_run, self.f_bytes + next_len as u64);
    }

    fn update_d(&mut self, next_len: usize) {
        self.deadline = rtsc_y2x(&self.d_run, self.f_bytes + next_len as u64);
    }
}

impl Clone for Class {
    fn clone(&self) -> Self {
        Class {
            in_use: self.in_use,
            leaf: self.leaf,
            parent: self.parent,
            index: self.index,
            qlimit: self.qlimit,
            qdrops: self.qdrops,
            eligible: self.eligible,
            deadline: self.deadline,
            vtime: self.vtime,
            vmin: self.vmin,
            vmax: self.vmax,
            voff: self.voff,
            pvoff: self.pvoff,
            vadj: self.vadj,
            vperiod: self.vperiod,
            pvperiod: self.pvperiod,
            f_bytes: self.f_bytes,
            r_bytes: self.r_bytes,
            f_isc: self.f_isc,
            r_isc: self.r_isc,
            u_isc: self.u_isc,
            f_run: self.f_run,
            e_run: self.e_run,
            d_run: self.d_run,
            u_run: self.u_run,
            nactive: self.nactive,
            children: BTreeMap::new(),
            packets: VecDeque::new(),
        }
    }
}

#[cfg(test)]
mod test;
