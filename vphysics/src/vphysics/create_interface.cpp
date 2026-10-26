#include "box3d_setup.h"
#include "physics.h"
#include "physics_collision.h"
#include "physics_nm.h"
#include "physics_surface_props.h"

#include "tier1/interface.h"

#include <cstring>

DLL_EXPORT void *CreateInterface( const char *pName, int *pReturnCode )
{
	SetupBox3D();

	void *pInterface = nullptr;
	if ( !std::strcmp( pName, VPHYSICS_INTERFACE_VERSION ) )
		pInterface = static_cast<IPhysics *>( &g_Physics );
	else if ( !std::strcmp( pName, VPHYSICS_COLLISION_INTERFACE_VERSION ) )
		pInterface = static_cast<IPhysicsCollision *>( &g_PhysicsCollision );
	else if ( !std::strcmp( pName, VPHYSICS_SURFACEPROPS_INTERFACE_VERSION ) )
		pInterface = static_cast<IPhysicsSurfaceProps *>( &g_PhysicsSurfaceProps );
	else if ( !std::strcmp( pName, VPHYSICS_NM_INTERFACE_VERSION ) || !std::strcmp( pName, VPHYSICS_NM_INTERFACE_VERSION_002 ) || !std::strcmp( pName, VPHYSICS_NM_INTERFACE_VERSION_001 ) )
		pInterface = static_cast<IPhysicsNM *>( &g_PhysicsNM );

	if ( pReturnCode )
		*pReturnCode = pInterface ? IFACE_OK : IFACE_FAILED;

	return pInterface;
}
