set( BOX3D_SOURCE_DIR "${CMAKE_SOURCE_DIR}/external/box3d" )
set( BOX3D_PATCHES "${CMAKE_SOURCE_DIR}/cmake/patches/box3d-joint-armature.patch" "${CMAKE_SOURCE_DIR}/cmake/patches/box3d-elliptical-cone.patch" )
set( BOX3D_DIR "${CMAKE_BINARY_DIR}/box3d" )

# Box3D is built from a patched copy in the build tree, the submodule stays untouched.
# The copy is refreshed when the submodule commit or a patch changes.
find_package( Git REQUIRED )
execute_process( COMMAND "${GIT_EXECUTABLE}" rev-parse HEAD WORKING_DIRECTORY "${BOX3D_SOURCE_DIR}" OUTPUT_VARIABLE BOX3D_COMMIT OUTPUT_STRIP_TRAILING_WHITESPACE ERROR_QUIET )
set( BOX3D_STAMP "${BOX3D_COMMIT}" )
foreach( BOX3D_PATCH ${BOX3D_PATCHES} )
	file( SHA256 "${BOX3D_PATCH}" BOX3D_PATCH_HASH )
	string( APPEND BOX3D_STAMP ";${BOX3D_PATCH_HASH}" )
	set_property( DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${BOX3D_PATCH}" )
endforeach()

set( BOX3D_STAMP_FILE "${BOX3D_DIR}/.vphysics-patched" )
set( BOX3D_OLD_STAMP "" )
if( EXISTS "${BOX3D_STAMP_FILE}" )
	file( READ "${BOX3D_STAMP_FILE}" BOX3D_OLD_STAMP )
endif()

if( NOT BOX3D_OLD_STAMP STREQUAL BOX3D_STAMP )
	message( STATUS "Patching Box3D into ${BOX3D_DIR}" )
	file( REMOVE_RECURSE "${BOX3D_DIR}" )
	file( COPY "${BOX3D_SOURCE_DIR}/CMakeLists.txt" "${BOX3D_SOURCE_DIR}/src" "${BOX3D_SOURCE_DIR}/include" DESTINATION "${BOX3D_DIR}" )
	foreach( BOX3D_PATCH ${BOX3D_PATCHES} )
		# the ceiling keeps git from treating the copy as part of this repository
		execute_process(
			COMMAND "${CMAKE_COMMAND}" -E env "GIT_CEILING_DIRECTORIES=${CMAKE_BINARY_DIR}" "${GIT_EXECUTABLE}" apply --ignore-whitespace --whitespace=nowarn "${BOX3D_PATCH}"
			WORKING_DIRECTORY "${BOX3D_DIR}"
			RESULT_VARIABLE BOX3D_PATCH_RESULT
		)
		if( NOT BOX3D_PATCH_RESULT EQUAL 0 )
			message( FATAL_ERROR "Failed to apply ${BOX3D_PATCH} to Box3D ${BOX3D_COMMIT}" )
		endif()
	endforeach()
	file( WRITE "${BOX3D_STAMP_FILE}" "${BOX3D_STAMP}" )
endif()

if( MSVC AND CMAKE_SIZEOF_VOID_P EQUAL 4 )
	include( ExternalProject )

	ExternalProject_Add( box3d_clang
		SOURCE_DIR "${BOX3D_DIR}"
		CMAKE_GENERATOR "${CMAKE_GENERATOR}"
		CMAKE_GENERATOR_PLATFORM "${CMAKE_GENERATOR_PLATFORM}"
		CMAKE_GENERATOR_INSTANCE "${CMAKE_GENERATOR_INSTANCE}"
		CMAKE_GENERATOR_TOOLSET ClangCL
		CMAKE_ARGS "-DCMAKE_PROJECT_INCLUDE=${CMAKE_SOURCE_DIR}/cmake/MsvcRuntime.cmake" -DBOX3D_SAMPLES=OFF -DBOX3D_UNIT_TESTS=OFF -DBOX3D_BENCHMARKS=OFF -DBOX3D_DOCS=OFF -DBOX3D_VALIDATE=OFF
		BUILD_ALWAYS ON
		INSTALL_COMMAND ""
		BUILD_BYPRODUCTS
			"<BINARY_DIR>/src/Release/box3d.lib"
			"<BINARY_DIR>/src/Debug/box3dd.lib"
	)
	ExternalProject_Get_Property( box3d_clang BINARY_DIR )

	add_library( box3d::box3d STATIC IMPORTED )
	set_target_properties( box3d::box3d PROPERTIES
		IMPORTED_CONFIGURATIONS "RELEASE;DEBUG"
		IMPORTED_LOCATION_RELEASE "${BINARY_DIR}/src/Release/box3d.lib"
		IMPORTED_LOCATION_DEBUG "${BINARY_DIR}/src/Debug/box3dd.lib"
		INTERFACE_INCLUDE_DIRECTORIES "${BOX3D_DIR}/include"
	)
	add_dependencies( box3d::box3d box3d_clang )
else()
	add_subdirectory( "${BOX3D_DIR}" "${CMAKE_BINARY_DIR}/box3d-build" EXCLUDE_FROM_ALL )
endif()
