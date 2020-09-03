#!/usr/bin/env bash

allowf=foobar
for i in `cat allow-function.regex`
do
   allowf="${allowf}|${i}"
done

allowt=foobar
for i in `cat allow-type.regex`
do
   allowt="${allowt}|${i}"
done

allowv=foobar
for i in `cat allow-var.regex`
do
   allowv="${allowv}|${i}"
done

bindgen headers.h --whitelist-function $allowf --whitelist-type $allowt --whitelist-var $allowv -o include/lib.rs
