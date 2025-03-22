---
weight: 1
type: docs
description: >

---

# CLIs using Rust Clap library

Command line argument parsing is a messy and complex affair, and we by now means want to spend time writing code for that. We use the Rust Clap library, which is quite popular, quite feature rich and quite easy to use, and fairly well documented - cant ask for more! And needless to say I did not have much troubles with clap. Clap has a few different styles of programming a CLI, again refer to Clap documents for that. R2 does not mandate any particular way of using clap - feel free to use what fits your need, but my preferred style of using clap is by defining CLIs using a YAML file.

YAML files can get notoriously complicated, but the goal we have is to define small independent stand-alone CLIs which define small YAML files - let a utility command do one thing, have multiple utils binaries to do different things. I dont have an alternate suggestion - unless the CLI is DAMN SIMPLE, the other Clap ways of defining the CLI using a chain of Rust objects etc.. seems far more complex to me than a YAML file, so YAML is the best option IMO, like it or not. And there are not any better/simpler Rust parsers other than Clap, and to be fair Clap is pretty good and I think its not too bad to have to live with this YAML option.

Again, the best way of adding a new CLI is by mimicking an existing one - and I will again quote the utils/r2log cli as an example because its really tiny. And for more complex examples, consult r2intf as an example, and play around with it by typing just "r2intf" and see the help strings that pop out, and try the subcommands and different combinations to get an idea of how things work.

The basic concepts in the Clap yaml file are below, and always remember that yaml is very particular about indendation - so just use a good editor with a yaml extention so that the indendation etc.. gets adjusted for you automatically

There is an 'args' keyword which basically means that "this is an argument", what follows is the name of the argument, and whether the argument is mandatory (required: true) or not (required:false). Inside the code, the value of that argument can be obtained by matches.value_of([argument name]). Also if the arg is mandatory and in a specific position, then you can choose to have the arg as a keyword-followed-by-value or just a value directly. For example "r2intf IFNAME" as seen in utils/r2intf/r2intf.yml is a mandatory argument and theres no key word for it .. You just type "r2intf eth0 [whatever else follows]" .. And in this case in r2intf/src/main.c, a call to matches.value_of("IFNAME") directly giveds the supplied interface name - eth0 in this example. If we want the keyword-value style of configurng, then the args has a 'long' / 'short' version of specifying the keyword. For example the args qlimit in rtintf.yml will be configured as 'r2intf eth0 [blah blah] --qlimit 100' - and in the code the qlimit will be retrieved as below

    if matches.is_present("qlimit") {
        qlimit = value_t!(matches, "qlimit", i32).unwrap_or_else(|e| e.exit());
    }

The value_t! macro is a convenient way to convert the text to a value of particular type in Rust. The other concept in the yaml file is a 'subcommand' - a subcommand is a choice between whether you want to excute A or B. For example in r2intf.yml, the options are 'add' and 'class' - so you can either add a new interface OR configure a qos class on an existing interface. Clap does not support multiple subcommands on the same line - at a time you can do only ONE of all the possible subcommands. 

A few other useful constructs are - 'takes_value' which says whether the keyword needs any value. Just the presence of a keyword like 'delete' is often sufficient to say what needs to be done without any value, so take_value can be false in that case. The other useful construct is 'requires' - you can say that if option A is configured, then option B and C also has to be configured, ie option A 'requires' B and C

And to emphasise what we mentoined initially again - keep the CLIs small and simple. If the yaml file grows too big, it will reach a stage where no one can figure out whats doing what and how to modify anything.
