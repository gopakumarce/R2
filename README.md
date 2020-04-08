# What is R2

R2 is a 'Router in Rust', born out of the desire to build a network packet forwarding engine using all the good concepts I have learned over my career spanning all areas of systems and computer networks, having learned from both proprietary and open source solutions. Disclaimer - R2 is a personal project of mine, nothing to do with my past or current employer(s). 

The concept of how a packet forwarding engine should look like, what its architecture should be etc.. was already baked in my mind from different experiences working in this domain. Then the choice to make was the language - I was sure it cant be C. It wont be an exaggeration if I say that approximately 20% of all large projects written in C spend their manpower in debugging memory overruns and double frees and stack corruptions and what not. It is just absurd if someone thinks of writing a system in C in this modern day and age.

## Why Rust

I started learning Go and Rust at about the same time, the core concepts of Rust felt an exact match to what I was wishing for in a systems language. I could naturally relate to why we shouldn't be allowed to mutate something when there are references pointing to it, why we cant just "send" some value to another thread if it has some pointers inside it etc.. etc.. I dropped Go and stuck to Rust. Not to paint an all rosy picture, Rust was NOT easy to learn, it took two to three months to get a hang of, after that there was no looking back. It really is a small language from a syntax/features perspective, so getting over that bump is easy. The hard part is getting used to programming with the memory concepts of Rust and the constraints it imposes.

If you are a good systems programmer, regardless of the language, you will spend a lot of time upfront mapping out every single detail in your system - how much memory is allocated and when, what memory is shared with references/refcounts, how are the threads interacting with each other, what are the locks and possible contentions, races etc.. If you have that level of details chalked out upfront, translating it to Rust is not hard - THAT is what Rust is designed to do. If you want to write random code with no thoughts behind it, that is not easy with Rust.

## Goals of R2

From a use case perspective, having R2 as a general purpose branch aggregation/service provider routing/switching engine is far away in terms of features etc... My hope is that it serves as an entry point that would be less 'feature hungry' like a vswitch in a virtual environment, maybe as a user space container networking alternative.

Regardless of the functionality, from an architecture perspective, what I considered when making any design/architecture choice in R2 is 3Ps - a Predictable, Performant, Pristine software.

Predictable: In most systems, we can answer questions like "how fast will the packet be re-routed in case of an interface failure" as long as there is a handful of interfaces. And once the number scales up, often the answer is "hard to predict". The goal with R2 is to have a design/architecture that keeps the behavior predictable all the time - the system might slow down with scale, but it should slow down predictably.

Performant: Obviously we are building a packet forwarding engine, it is no good if it is not performant.

Pristine: Coding is a collaborative effort, and the cleaner and simpler the code is, the better the end result. If some piece of code looks like black magic no matter how 'smart' it is, it needs to be thrown out. If some feature is not in use, it needs to be thrown out. Code periodically needs maintenance and rework - and to support that goal, it should have a solid test framework that allows people to make changes with confidence.

## What can R2 do today

Functionally / feature-wise, R2 is a new born; we can just add ethernet interfaces and routes to R2 and it will do basic arp resolution and IPv4 forwarding, that is about it (Oh, and of course an HFSC hierarchical qos scheduler). But from an architecture and infrastructure perspective, R2 has a solid foundation that will allow people to start contributing with clear guidelines on how to extend R2. Read the Architecture section to know more. The positive spin of being functionally tiny and architecturally well thought out is: I believe that is exactly what excites people to contribute - it is more exciting to build something starting when it is tiny than contributing to a behemoth.

## Getting familiar with R2

The recommended method is to first go through the Architecture page <https://r2.rs/architecture/>, which is a high level overview of R2. And once that is done, go through the TryMe page <https://r2.rs/tryme/> and just get familiar with downloading the code, compiling it and getting R2 running with a simple two container setup. Once that is done go through the Code page <https://r2.rs/code/> to zoom a bit more into a summary of how the code in different modules work, and at that point you can refer to the code itself and go through the comments in the code. All the documentation in this website is present as markdown files in the code repository itself.

## Issues / Feature tracking

See the issues section in R2's github page to see the list of bugs/enhancements to be taken up immediately

## The Logo

The logo depicts a macrophage. It was a funny-looking stuffed toy in my wife's microbiology lab, which turned into this cartoon version. To try and give it a software spin (as an afterthought), macrophages are constantly cleaning up "bad stuff" from a biological system - and that aligns well with the being 'Pristine'  goal of R2.  

The logo is created by the talented graphics artist Shalaka <https://www.behance.net/shalakasdesign>. I tried to draw one myself and after a days worth of effort even my very basic version went nowhere. I quickly realized how much of artistic talent and skill one needs to create a logo. It was amazing to see the dexterity with which Shalaka created multiple versions of the logo in no time!
