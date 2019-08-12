# How to write an API

So here I will not talk about how to write thrift APIs or its syntax etc.. There are plenty of documentation on that in google. Once the thrift file is written, to generate rust code we have to run "thrift -out [directory] --gen rs -r [thrift API file]". This step and how to build the thrift compiler etc.. are all available documentation in google.

To write a new API for your module foo, here are the steps. I would use the code in apis/log/ and utils/r2log as simple cookie cutter templates. Outlining the broad steps here

1. We are putting all the thrift API files in the apis/ directory. So create an apis/foo/ with its Cargo.toml and all that.

2. Define the thrift API file apis/foo/src/apis.thrift and put APIs in it. Say you define "service Foo" in this file where the Foo {} will have your APIs like whatever_api_i_defined()

3. Compile the apis.thrift to a rust file and put it in apis/foo/src/lib.rs. At some point we will convert this step to be automatically done when we do a cargo build

4. In common define a name common::FOO_APIS for your API service (like common::LOG_APIs for the logger)

5. Inside R2, define an object for your module that has implementations for all the APIs that your module exposes, and whatever other data your module wants to put in there (similar to main/src/log.rs::LogApis). Lets call it FooCtx. And similar to the log example, implement the required trait "impl FooSyncHandler for FooCtx" - which will have all the server side callbacks for the services/APIs you defined in step 2.

6. In register_apis() inside R2, register the FooCtx just like the logger does it - svr.register(common::FOO_APIS, Box::new(FooSyncProcessor::new(foo_ctx))

7. In the external rust utility where you want to invoke the R2 API, call api::api_client() to open a channel to talk to R2.

8. Using the channel above, create a client context for your service - FooSyncClient::new(i_prot, o_prot) - lets call it foo_ctx_client. And then using this context you can call all the APIs you defined in the thrift file in step 2 - you can call foo_ctx_client.whatever_api_i_defined()

Once you have your basic API defined along the lines of the log/ example and you get it compiled and working, its easier to then do more fancy stuff by consulting the other apis in the code like apis/interface or apis/route which have more flavours like error handling and returning error codes etc..
