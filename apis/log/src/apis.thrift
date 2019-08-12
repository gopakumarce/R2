exception LogErr {
  1: string why
}

service Log {
    void show(1:string filename) throws (1:LogErr ouch),
}