# How to generate dpdk ffi binding APIs

* install dpdk - using meson build and ninja install etc..

* temporarilt overwrite rte_memcpy.h as below, restore it when these steps are complete
   sudo cp ./lib/librte_eal/common/include/generic/rte_memcpy.h /usr/local/include/rte_memcpy.h

* Open /usr/local/include/rte_ether.h and in struct rte_ether_addr remove attribute aligned,
   that causes issues with bindgen and transitive repr(aligned) inclusions

* Add the headers you want to ./headers.h

* now run ./bindgen.sh

