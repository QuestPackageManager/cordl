use crate::generate::metadata::CordlMetadata;

use super::cpp_context_collection::CppContextCollection;

pub struct CppNameResolver<'a> {
    pub metadata: &'a CordlMetadata<'a>,
    pub collection: &'a CppContextCollection,

}