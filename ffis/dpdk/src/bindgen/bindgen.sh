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

bindgen headers.h --raw-line "#![allow(clippy::all)]" --raw-line "#![allow(dead_code)]" --allowlist-function $allowf --allowlist-type $allowt --allowlist-var $allowv -o include/lib.rs
